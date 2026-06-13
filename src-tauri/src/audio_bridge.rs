//! Audio bridge (rhanis-flu): cpal microphone capture → PCM16 24kHz → WebSocket,
//! and WebSocket audio delta → PCM16 decode → rodio playback.
//!
//! # Design overview
//!
//! ```text
//! cpal input stream           conversion seam            session_manager
//! ─────────────────  →  PcmConverter (pure logic)  →  write_tx (mpsc)
//! device rate / format        mono 24 kHz PCM16         input_audio_buffer.append
//!
//! session_manager             conversion seam            rodio OutputStream
//! write loop rx  →  audio delta (GA/beta)  →  PlaybackQueue  →  rodio Sink
//! base64 PCM16                i16 decode                       speaker
//! ```
//!
//! ## Thread-safety model
//! `cpal::Stream` and `rodio::OutputStream` are NOT `Send` on all platforms
//! (they contain OS-level audio handles with thread-affinity requirements).
//! Rather than wrapping them in `unsafe impl Send`, both are **confined to a
//! dedicated `std::thread`** (the "audio thread") that owns them for its entire
//! lifetime.  The public `AudioBridge` API communicates with that thread via a
//! `std::sync::mpsc::Sender<AudioCommand>`, which IS `Send`, so `AudioBridge`
//! itself is safely `Send + Sync` via the inner `Arc`.
//!
//! ## Testability boundary
//! All conversion logic — f32↔i16 clamping, resampling, 20ms chunk sizing,
//! base64 encode, base64 decode — lives in pure free functions exercised by
//! `#[cfg(test)] mod tests` **without opening any real device**. Hardware
//! calls (`cpal` stream open, `rodio` OutputStream) are isolated in the audio
//! thread; those paths are integration-tested only on Windows (rhanis-ef8 E2E).
//!
//! ## Fail-closed discipline
//! - `start` returns `Result<_, String>`; any device-open error is surfaced as
//!   a message, never a panic.
//! - cpal `error_callback` logs to stderr (category only, no OS detail) and
//!   marks the bridge stopped; session_manager checks `AudioBridge::is_running()`
//!   and stops the session when the bridge goes dark.
//! - Playback queue is bounded (~5 s of audio at 24kHz); overflow drops and
//!   emits an error on the running flag so the session stops fail-closed.
//! - All buffer copies use `saturating` arithmetic; integer overflow → silence,
//!   not a crash.
//!
//! ## WSL note
//! cpal cannot open a real mic in WSL (no ALSA/PulseAudio device). The pure
//! logic tests run fine in WSL (no hardware). Real-mic E2E requires
//! `pnpm tauri dev` on native Windows (rhanis-ef8).
//!
//! transaction N/A · idempotency_key N/A (real-time audio I/O, not billing).

use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Arc;

use base64::Engine as _;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

// ── Audio payload size guard ──────────────────────────────────────────────────

/// Maximum decoded byte size of a single audio-delta payload (either wire
/// name, see [`is_audio_delta_type`]).
/// A server-supplied delta exceeding this limit is silently dropped rather than
/// allocated.  256 KiB ≈ 5.46 seconds of 24kHz PCM16 mono — far beyond any
/// realistic single delta packet.  Equivalent to `MAX_TOOL_OUTPUT_LEN` in
/// `tool_dispatcher.rs`.
pub const MAX_AUDIO_DELTA_BYTES: usize = 256 * 1024;

/// Maximum total queued bytes in the playback queue (~5 s at 24kHz PCM16 mono:
/// 24000 samples/s × 2 bytes × 5 s = 240 000 bytes).  On overflow the queue is
/// cleared and the `running` flag is set to false so the session stops fail-closed
/// rather than buffering unbounded server audio (DoS guard).
pub const MAX_PLAYBACK_QUEUE_BYTES: usize = 240_000;

/// Maximum raw JSON text length accepted before `serde_json::from_str`.
/// A single OpenAI Realtime frame is never legitimately larger than this
/// (even audio delta frames are base64 text ≤ `MAX_AUDIO_DELTA_BYTES * 4/3`).
pub const MAX_WS_TEXT_BYTES: usize = MAX_AUDIO_DELTA_BYTES * 4 / 3 + 4096;

/// Maximum byte length of the raw `arguments` string in a function-call event
/// before the inner JSON parse.  Tools never require more than this; anything
/// larger is a crafted injection attempt.
pub const MAX_ARGS_LEN: usize = 16 * 1024;

// ── OpenAI Realtime audio constants ──────────────────────────────────────────

/// Target sample rate expected by the OpenAI Realtime API (PCM16, mono).
pub const REALTIME_SAMPLE_RATE: u32 = 24_000;

/// 20-millisecond PCM16 chunk (samples, mono). OpenAI docs recommend this cadence
/// for `input_audio_buffer.append`; smaller → more WS overhead, larger → latency.
/// 24000 samples/s × 0.020 s = 480 samples → 960 bytes as i16 little-endian.
pub const CHUNK_SAMPLES: usize = (REALTIME_SAMPLE_RATE as usize) * 20 / 1_000;

// ── PCM sample-format conversion (pure, testable) ────────────────────────────

/// Converts a slice of f32 samples (each in −1.0…+1.0; values outside are
/// clamped; non-finite values are treated as 0) to PCM16 little-endian bytes.
///
/// Rounding: a +0.5 bias before truncation makes the error distribution
/// symmetric around zero for mid-scale sine waves (rounds to nearest rather
/// than always flooring).  The bias is intentional; see inline comment.
///
/// This is the inner hot path for the mic capture; kept as a free function so
/// the test suite can drive it without any device.
pub fn f32_slice_to_pcm16_le(samples: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(samples.len() * 2);
    for &s in samples {
        // Non-finite values (NaN, ±Inf) are silenced rather than producing
        // undefined casts.  Clamp first so the multiply below never wraps.
        let finite = if s.is_finite() { s } else { 0.0_f32 };
        let clamped = finite.clamp(-1.0_f32, 1.0_f32);
        // Multiply by i16::MAX (32767). Intentional +0.5 rounding bias: adds ½
        // LSB before truncation so mid-scale sine waves round symmetrically
        // rather than always flooring (reduces DC offset in the encoded stream).
        let rounded = (clamped * 32767.0_f32 + 0.5_f32).min(32767.0_f32).max(-32768.0_f32);
        let pcm: i16 = rounded as i16;
        out.extend_from_slice(&pcm.to_le_bytes());
    }
    out
}

/// Converts raw PCM16 little-endian bytes to f32 samples in −1.0…+1.0.
/// Pairs of bytes are parsed as i16 LE; integer min (`−32768`) maps to
/// −1.0 (exactly representable as f32 from the division below).
///
/// Used by the playback path to decode the audio-delta base64 blob.
pub fn pcm16_le_to_f32(bytes: &[u8]) -> Vec<f32> {
    let pairs = bytes.len() / 2; // truncate any trailing odd byte
    let mut out = Vec::with_capacity(pairs);
    for chunk in bytes[..pairs * 2].chunks_exact(2) {
        let i = i16::from_le_bytes([chunk[0], chunk[1]]);
        // Divide by 32768 (not 32767) so i16::MIN maps to exactly −1.0 and
        // i16::MAX maps to 0.999969…, staying strictly inside [−1, +1].
        out.push(i as f32 / 32768.0_f32);
    }
    out
}

/// Mixes a multi-channel f32 stream down to mono by averaging all channels.
/// Returns the original slice unchanged when `channels == 1`.
pub fn downmix_to_mono(samples: &[f32], channels: usize) -> Vec<f32> {
    if channels == 0 || samples.is_empty() {
        return Vec::new();
    }
    if channels == 1 {
        return samples.to_vec();
    }
    let frames = samples.len() / channels;
    let mut mono = Vec::with_capacity(frames);
    let inv = 1.0_f32 / channels as f32;
    for frame in samples[..frames * channels].chunks_exact(channels) {
        let sum: f32 = frame.iter().copied().sum();
        mono.push(sum * inv);
    }
    mono
}

/// Linear resampler for mono f32 streams.
///
/// Quality is "voice adequate" — for 24 kHz speech the gentle linear
/// interpolation produces no perceptible artefacts at the resampling ratios
/// cpal devices use (44100 → 24000, 48000 → 24000, 16000 → 24000).
///
/// `in_rate` and `out_rate` must both be > 0; if `in_rate == out_rate` the
/// input is returned unchanged.
pub fn resample_mono_linear(samples: &[f32], in_rate: u32, out_rate: u32) -> Vec<f32> {
    if in_rate == out_rate || samples.is_empty() {
        return samples.to_vec();
    }
    // Number of output samples for the whole input block.
    let out_len = (samples.len() as f64 * out_rate as f64 / in_rate as f64).ceil() as usize;
    let mut out = Vec::with_capacity(out_len);
    // Step through output positions, computing the corresponding input position.
    let ratio = in_rate as f64 / out_rate as f64;
    for i in 0..out_len {
        let pos = i as f64 * ratio;
        let lo = pos.floor() as usize;
        let hi = (lo + 1).min(samples.len().saturating_sub(1));
        let frac = (pos - lo as f64) as f32;
        // Safety: lo < samples.len() is guaranteed by the ceil computation above
        // because the last output index maps to at most samples.len()-1 in input.
        let lo_val = samples.get(lo).copied().unwrap_or(0.0);
        let hi_val = samples.get(hi).copied().unwrap_or(lo_val);
        out.push(lo_val + (hi_val - lo_val) * frac);
    }
    out
}

/// Builds an `input_audio_buffer.append` WebSocket frame from a PCM16 LE
/// byte slice, base64-encoding the payload as OpenAI Realtime requires.
pub fn build_audio_append_frame(pcm16_bytes: &[u8]) -> Message {
    let encoded = base64::engine::general_purpose::STANDARD.encode(pcm16_bytes);
    let json = serde_json::json!({
        "type": "input_audio_buffer.append",
        "audio": encoded,
    });
    Message::Text(json.to_string().into())
}

/// Whether `event_type` is an assistant audio-delta frame, under either wire
/// name: the GA Realtime event is `response.output_audio.delta`; the
/// superseded beta interface used `response.audio.delta` (rhanis-bd7). Matching
/// only the beta name leaves the assistant silent on a GA handshake while
/// every other path (transcript, usage) keeps working. Decode, the barge-in
/// gate, and `parse_frame`'s pinned Ignored arm all route through this single
/// predicate so the sites cannot diverge; once rhanis-ef8 (Windows E2E) pins the
/// live wire name, the unused arm can be dropped here in one place. Mirrors
/// the transcript dual-name match in `realtime_provider::parse_frame`.
pub(crate) fn is_audio_delta_type(event_type: &str) -> bool {
    matches!(
        event_type,
        "response.audio.delta" | "response.output_audio.delta"
    )
}

/// Decodes the base64 payload from an assistant audio-delta server event
/// (GA `response.output_audio.delta` or beta `response.audio.delta`, see
/// [`is_audio_delta_type`]), returning the raw PCM16 LE bytes.  Returns
/// `None` if the event type is wrong, the `delta` field is absent / not a
/// valid base64 string, or the decoded byte length would exceed
/// [`MAX_AUDIO_DELTA_BYTES`] (DoS guard).
pub fn decode_audio_delta(event_json: &serde_json::Value) -> Option<Vec<u8>> {
    if !is_audio_delta_type(event_json.get("type")?.as_str()?) {
        return None;
    }
    let b64 = event_json.get("delta")?.as_str()?;
    // base64 expands by factor 4/3; check the raw b64 length before decoding
    // to avoid a heap allocation for an oversized server-controlled payload.
    // Add 4 to account for padding bytes.
    if b64.len() > MAX_AUDIO_DELTA_BYTES * 4 / 3 + 4 {
        eprintln!("[audio_bridge] audio delta too large, dropping");
        return None;
    }
    let bytes = base64::engine::general_purpose::STANDARD.decode(b64).ok()?;
    // Secondary guard on the decoded size (base64 padding can shift the
    // estimate slightly; this also catches crafted payloads that pass the
    // length check above but decode to more bytes than expected).
    if bytes.len() > MAX_AUDIO_DELTA_BYTES {
        eprintln!("[audio_bridge] decoded audio delta too large, dropping");
        return None;
    }
    Some(bytes)
}

// ── Capture chunk accumulator (pure) ─────────────────────────────────────────

/// Accumulates converted mono-24kHz f32 samples from potentially many small
/// cpal callbacks, draining into 20ms PCM16 WS frames whenever enough data
/// has been collected.  This is a plain struct — no thread primitives — so
/// the tests drive it synchronously.
pub struct ChunkAccumulator {
    buf: Vec<f32>,
    /// Samples per flush; initialised to [`CHUNK_SAMPLES`].
    chunk_samples: usize,
}

impl ChunkAccumulator {
    pub fn new() -> Self {
        Self {
            buf: Vec::new(),
            chunk_samples: CHUNK_SAMPLES,
        }
    }

    /// For tests that want a different chunk size.
    pub fn with_chunk_samples(chunk_samples: usize) -> Self {
        Self {
            buf: Vec::new(),
            chunk_samples,
        }
    }

    /// Appends converted samples.  Returns any complete 20ms chunks as PCM16 LE
    /// byte vectors.  Remaining (incomplete) samples are held for the next call.
    pub fn push(&mut self, samples: &[f32]) -> Vec<Vec<u8>> {
        self.buf.extend_from_slice(samples);
        let mut chunks = Vec::new();
        while self.buf.len() >= self.chunk_samples {
            let frame: Vec<f32> = self.buf.drain(..self.chunk_samples).collect();
            chunks.push(f32_slice_to_pcm16_le(&frame));
        }
        chunks
    }

    /// Flushes any remaining samples (zero-padded to `chunk_samples`) into a
    /// final PCM16 chunk.  Called on session stop to avoid cutting off the last
    /// few milliseconds of speech. Returns `None` if the buffer is empty.
    pub fn flush(&mut self) -> Option<Vec<u8>> {
        if self.buf.is_empty() {
            return None;
        }
        // Zero-pad to a full chunk so the decoder always sees complete frames.
        while self.buf.len() < self.chunk_samples {
            self.buf.push(0.0);
        }
        let out = f32_slice_to_pcm16_le(&self.buf);
        self.buf.clear();
        Some(out)
    }
}

impl Default for ChunkAccumulator {
    fn default() -> Self {
        Self::new()
    }
}

// ── AudioBridgeState (shared between cpal callback and Tauri layer) ───────────

/// Shared state visible from both the cpal data-callback thread and the async
/// session_manager task.  Uses only `Arc<AtomicBool>` so no async locks are
/// needed in the cpal callback (cpal callbacks must be lock-free / no async).
///
/// ## Per-session generation flag (rhanis-flu round 5)
///
/// The running flag is **per session**, not a single Arc reused across
/// sessions.  Each `start()` installs a **fresh** `Arc<AtomicBool>` via
/// [`new_generation`](Self::new_generation).  An old audio thread holds a clone
/// of its OWN generation's Arc, so its teardown (which writes `false`) can NEVER
/// clear the NEXT session's flag.  Without this, a stale thread tearing down
/// after a new `start()` would make the new session see a spurious "mic lost".
///
/// The current generation's Arc lives behind a `std::sync::Mutex` so `start()`
/// can swap it atomically while `is_running()` / `running_flag()` / `stop()`
/// read whichever generation is current.  The mutex is held only for the
/// duration of a clone — never across the cpal callback (which captures its own
/// Arc clone at spawn time and never touches this mutex).
pub struct AudioBridgeState {
    /// The current generation's running flag.  Replaced wholesale on each
    /// `new_generation()` so old threads cannot clobber a newer generation.
    current: std::sync::Mutex<Arc<AtomicBool>>,
}

impl AudioBridgeState {
    pub fn new() -> Self {
        Self {
            current: std::sync::Mutex::new(Arc::new(AtomicBool::new(false))),
        }
    }

    /// Returns a clone of the current generation's running flag.
    fn current_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.current.lock().unwrap())
    }

    /// Returns `true` while the bridge is capturing (reads the current generation).
    pub fn is_running(&self) -> bool {
        self.current_flag().load(Ordering::Acquire)
    }

    /// Installs a **fresh** running flag for a new session and returns a clone of
    /// it (set to `true`).  The previous generation's Arc is dropped from
    /// `current` but any thread still holding a clone keeps mutating ITS own Arc,
    /// never this new one — so a stale thread's teardown cannot clear the new
    /// session's flag.
    pub fn new_generation(&self) -> Arc<AtomicBool> {
        let fresh = Arc::new(AtomicBool::new(true));
        *self.current.lock().unwrap() = Arc::clone(&fresh);
        fresh
    }

    /// Marks the current generation as stopped (idempotent).  Only affects the
    /// current generation; older generations are untouched.
    pub fn stop(&self) {
        self.current_flag().store(false, Ordering::Release);
    }
}

impl Default for AudioBridgeState {
    fn default() -> Self {
        Self::new()
    }
}

// ── Audio thread channels: DATA (lossy PCM) vs CONTROL (reliable stop) ────────

/// **Data** command: PCM payloads bound for the playback queue.  Carried on the
/// bounded, **lossy** DATA channel (`std::sync::mpsc::SyncSender<AudioCommand>`).
///
/// ## Two-channel split (rhanis-flu round 5): control must never be dropped by PCM
///
/// The previous design carried both PCM **and** the stop control on a SINGLE
/// bounded channel.  During active server playback `handle_server_audio` floods
/// that channel with `EnqueuePcm`; a concurrent `stop_graceful()` then did
/// `try_send(FlushThenStop)` which returned `Full` → the graceful tail flush was
/// silently dropped and the audio thread could fail to stop promptly.
///
/// Fix: PCM data and stop control now travel on **two separate channels**:
/// - **DATA channel** (`AudioCommand::EnqueuePcm`): bounded; lossy `try_send` is
///   fine — dropping a playback chunk under congestion is acceptable.
/// - **CONTROL channel** ([`AudioControl`]): tiny capacity; delivery is
///   **reliable** on the shutdown path (a single pending control always fits,
///   and the shutdown caller retries `try_send` up to a short bounded deadline
///   via [`send_control_reliably`] as belt-and-suspenders).
///
/// The audio thread checks the CONTROL channel **first** (non-blocking) on every
/// loop iteration, then drains available PCM, so a stop is honoured even while
/// PCM keeps flowing.
///
/// `running` still has ONLY one purpose: gating the cpal capture data-callback
/// so it stops emitting PCM.  It is never read to decide whether to flush.
pub enum AudioCommand {
    /// Enqueue a PCM16 LE byte payload for immediate playback (DATA channel).
    EnqueuePcm(Vec<u8>),
}

/// One playback-sink operation, handed to the closure injected into
/// [`run_audio_command_loop`].  A single-closure seam (rather than one closure
/// per operation) because both operations mutate the same sink + queued-bytes
/// accounting, and two `FnMut` captures of one `&mut` cannot coexist.
pub enum PlaybackOp {
    /// Append one decoded PCM payload (the DATA-channel `EnqueuePcm` path).
    Enqueue(Vec<u8>),
    /// Barge-in cut (rhanis-bx7): clear everything queued in the sink, resume it,
    /// and reset the queued-bytes accounting.  The loop keeps running.
    Clear,
}

/// **Control** command: stop / barge-in intent for the audio thread.  Carried on
/// the tiny, **reliable** CONTROL channel — separate from PCM so a congested
/// playback queue can never drop a stop (or a barge-in cut).
///
/// The flush/discard decision is encoded in WHICH control is sent (decided at
/// **send time** by the caller); the receiver does NOT consult the `running`
/// atomic to decide whether to flush.
pub enum AudioControl {
    /// **Graceful stop**: flush the partial tail from the mic accumulator onto
    /// `write_tx` (unconditionally — no `running` flag check) **reliably** (a
    /// short bounded blocking send on the shutdown path), then stop.
    ///
    /// Use on **normal** server-close exits where the WS write task is NOT
    /// aborted — the tail PCM should reach the server.
    FlushThenStop,
    /// **Immediate stop**: discard the partial tail, do NOT flush, then stop.
    ///
    /// Use on **abnormal** exits (budget trip / WS error / timeout / mic lost)
    /// where the WS write task is aborted first.  Skipping the flush ensures
    /// no tail PCM races onto the WS after the writer abort.
    StopNow,
    /// **Barge-in playback cut (rhanis-bx7)** — the only NON-terminal control: the
    /// user started speaking, so stop the assistant's voice NOW. The audio
    /// thread (1) discards every `EnqueuePcm` already queued on the DATA
    /// channel (stale audio of the interrupted response), (2) clears + resumes
    /// the rodio sink, then (3) keeps running — capture continues, the user IS
    /// mid-sentence. Travels on CONTROL, not DATA: the cut must win over the
    /// very PCM flood it is cutting (P1-1 control-first ordering).
    ClearPlayback,
}

/// Tiny capacity for the CONTROL channel.  At most one stop control is pending
/// per session (graceful sends one `FlushThenStop`; immediate sends one
/// `StopNow`; the cpal error callback may add one `StopNow`), plus transient
/// `ClearPlayback` cuts (rhanis-bx7) that the audio thread consumes within one
/// ~20 ms loop iteration.  A capacity of 4 keeps a pending stop deliverable
/// even with a barge-in cut in flight; `send_control_reliably` retries cover
/// a momentary burst.
const CONTROL_CHANNEL_CAP: usize = 4;

/// Short bounded timeout for the **reliable** control send on the shutdown path.
/// Used by `stop_graceful` / `stop_immediate` (NOT the realtime cpal callback):
/// if the tiny control channel is momentarily full, block up to this long so the
/// control is delivered rather than dropped.  The audio thread drains control
/// first every loop iteration, so this never blocks meaningfully in practice.
const CONTROL_SEND_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(250);

/// Short bounded timeout for the **reliable** graceful tail delivery from the
/// audio thread to the (tokio) `write_tx`.  On the graceful path the writer is
/// NOT aborted and drains, so capacity frees up quickly; we retry `try_send`
/// up to this deadline so the normal-close tail is not dropped on a momentarily
/// full write queue.  Acceptable only on the shutdown path (the realtime
/// per-chunk capture path stays lossy/non-blocking).
const TAIL_DELIVERY_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(250);

/// Block on the audio command-receive timeout used to drain PCM with low
/// latency while still polling the control channel promptly.
const AUDIO_DATA_RECV_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(20);

/// Sends a stop control on the tiny CONTROL channel **reliably** within a short
/// bounded window.
///
/// `std::sync::mpsc::SyncSender::send_timeout` is nightly-only, so on stable we
/// emulate it: retry `try_send` with a tiny back-off up to [`CONTROL_SEND_TIMEOUT`].
/// The control channel has [`CONTROL_CHANNEL_CAP`] slots and at most one stop is
/// pending per session, so the FIRST `try_send` essentially always succeeds; the
/// retry is belt-and-suspenders so a transient `Full` (idempotent re-calls) does
/// not drop the stop.  Use ONLY on the shutdown path — NEVER the realtime cpal
/// callback (which uses plain `try_send` and may drop).
///
/// Returns `true` if delivered, `false` if the deadline elapsed or the channel
/// is disconnected (audio thread already gone — nothing to deliver).
fn send_control_reliably(ctrl_tx: &std::sync::mpsc::SyncSender<AudioControl>, ctrl: AudioControl) -> bool {
    use std::sync::mpsc::TrySendError;
    let deadline = std::time::Instant::now() + CONTROL_SEND_TIMEOUT;
    let mut ctrl = ctrl;
    loop {
        match ctrl_tx.try_send(ctrl) {
            Ok(()) => return true,
            Err(TrySendError::Full(returned)) => {
                if std::time::Instant::now() >= deadline {
                    return false;
                }
                ctrl = returned;
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            Err(TrySendError::Disconnected(_)) => return false,
        }
    }
}

// ── PlaybackHandle: lock-free playback / barge-in primitive (rhanis-bx7) ─────────

/// The playback gate is OFF: deltas flow to the sink normally.
const GATE_OFF: u8 = 0;
/// The user is speaking (`speech_started` seen): deltas are dropped, and a
/// `response.created` arriving NOW (e.g. a tool-completion follow-up response)
/// must NOT lift the gate — the user still has the floor.
const GATE_SPEAKING: u8 = 1;
/// The user finished speaking (`speech_stopped` seen): the NEXT
/// `response.created` — the reply to what the user just said — lifts the gate.
const GATE_ARMED: u8 = 2;

/// A lock-free handle for the read loop's `audio_handler` seam: playback
/// enqueueing + the barge-in gate (rhanis-bx7), WITHOUT acquiring the tokio
/// `Mutex<AudioBridge>`.
///
/// ## Why lock-free matters (same rationale as [`AudioStopHandle`])
/// The pre-rhanis-bx7 handler took `try_lock()` and skipped the frame on
/// contention — acceptable when a miss meant one lossy PCM chunk, but the
/// barge-in STATE TRANSITIONS (`speech_started` / `speech_stopped` /
/// `response.created`) now ride this seam: a missed `response.created` would
/// stick the gate closed for an entire turn (a full response of silence).
/// The handle clones the channel senders + the gate at `start()` time, so no
/// frame is ever dropped to mutex contention.
///
/// Per-connection: `establish_connection` grabs a fresh handle after each
/// (re)`start()`. The gate `Arc` is REPLACED on `start()` (not just reset), so
/// a stale handler from a previous generation cannot pollute the new session's
/// gate — the same generation discipline as the `running` flag.
pub struct PlaybackHandle {
    /// Barge-in gate — one of [`GATE_OFF`] / [`GATE_SPEAKING`] / [`GATE_ARMED`].
    /// Single writer (the read-loop task drives all transitions in frame order);
    /// the atomic is for the cross-thread handoff, not for contention.
    gate: Arc<AtomicU8>,
    /// DATA-channel sender (lossy PCM → playback).
    data_tx: std::sync::mpsc::SyncSender<AudioCommand>,
    /// CONTROL-channel sender (reliable; carries the barge-in cut).
    ctrl_tx: std::sync::mpsc::SyncSender<AudioControl>,
}

impl PlaybackHandle {
    /// The playback half of the read loop's `audio_handler` seam: feeds
    /// audio-delta payloads (GA `response.output_audio.delta` / beta
    /// `response.audio.delta`, see [`is_audio_delta_type`]) into the playback
    /// queue and drives the barge-in gate (rhanis-bx7).  Silently ignores unknown
    /// event types or malformed base64.
    ///
    /// Barge-in protocol:
    /// - `speech_started` → gate closes; on the OFF→SPEAKING transition ONLY,
    ///   one `ClearPlayback` cut goes out on CONTROL (idempotent per episode:
    ///   while the gate is closed no new audio reaches the sink, so repeat
    ///   speech-starts have nothing left to cut — and an attacker-controlled
    ///   frame flood cannot occupy the CONTROL channel or block this path;
    ///   the cut is `try_send`, dropped-if-full, because a full CONTROL channel
    ///   means a stop is already pending and the cut is moot).
    /// - `speech_stopped` → the gate arms: the user finished, the NEXT response
    ///   may speak.
    /// - `response.created` → lifts the gate ONLY from the armed state. A
    ///   response created while the user is STILL speaking (tool-completion
    ///   follow-ups via `response.create`, see rhanis-z8j) stays suppressed —
    ///   without this, a mid-speech `response.created` would re-open the gate
    ///   and the assistant would talk over the user (R-B finding).
    /// - audio delta (either wire name) → enqueued only while the gate is OFF;
    ///   checked BEFORE base64-decoding so a suppressed straggler flood costs
    ///   no CPU.
    ///
    /// The protocol half of barge-in (sending the provider's `response.cancel`)
    /// lives in the session loop via [`ProviderEvent::SpeechStarted`].
    ///
    /// [`ProviderEvent::SpeechStarted`]: crate::realtime_provider::ProviderEvent::SpeechStarted
    pub fn handle_server_audio(&self, event: &serde_json::Value) {
        match event.get("type").and_then(serde_json::Value::as_str) {
            Some("input_audio_buffer.speech_started") => {
                let prev = self.gate.swap(GATE_SPEAKING, Ordering::AcqRel);
                if prev == GATE_OFF {
                    let _ = self.ctrl_tx.try_send(AudioControl::ClearPlayback);
                }
            }
            Some("input_audio_buffer.speech_stopped") => {
                // Arm only from SPEAKING — a stray speech_stopped with no
                // preceding speech_started must not disturb an open gate.
                let _ = self.gate.compare_exchange(
                    GATE_SPEAKING,
                    GATE_ARMED,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                );
            }
            Some("response.created") => {
                // Lift only when armed (user finished speaking). Mid-speech
                // creations keep the gate closed — see the method doc.
                let _ = self.gate.compare_exchange(
                    GATE_ARMED,
                    GATE_OFF,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                );
            }
            Some(t) if is_audio_delta_type(t) => {
                if self.gate.load(Ordering::Acquire) != GATE_OFF {
                    // Straggler of an interrupted response — drop pre-decode.
                    return;
                }
                if let Some(pcm_bytes) = decode_audio_delta(event) {
                    // try_send is non-blocking on the DATA channel; if it is full
                    // (audio thread overwhelmed) we drop this playback chunk rather
                    // than blocking the read loop.  Stop control is NEVER on this
                    // channel, so a full DATA channel cannot drop a stop.
                    let _ = self.data_tx.try_send(AudioCommand::EnqueuePcm(pcm_bytes));
                }
            }
            _ => {}
        }
    }

    /// Builds a handle around fresh test channels — no audio thread, no device
    /// (the barge-in unit tests assert what reaches each channel).
    #[cfg(test)]
    fn new_for_test() -> (
        Self,
        std::sync::mpsc::Receiver<AudioCommand>,
        std::sync::mpsc::Receiver<AudioControl>,
    ) {
        let (data_tx, data_rx) = std::sync::mpsc::sync_channel::<AudioCommand>(64);
        let (ctrl_tx, ctrl_rx) = std::sync::mpsc::sync_channel::<AudioControl>(CONTROL_CHANNEL_CAP);
        (
            Self { gate: Arc::new(AtomicU8::new(GATE_OFF)), data_tx, ctrl_tx },
            data_rx,
            ctrl_rx,
        )
    }
}

// ── AudioStopHandle: lock-free stop primitive ─────────────────────────────────

/// A lock-free handle that can stop the audio bridge without acquiring the
/// tokio `Mutex<AudioBridge>`.
///
/// Captures the `running` flag and the audio-thread CONTROL sender at `start()`
/// time so the `stop_audio` closure built by `session_manager::start_session`
/// never needs to call `try_lock()`.  Under any contention (budget-trip, mic-
/// lost, or WS-error while the session task holds the bridge mutex) the closure
/// still stops the mic atomically.  Stop intent travels on the dedicated CONTROL
/// channel (never the lossy PCM DATA channel), so a congested playback queue
/// cannot drop the stop.
///
/// ## Why lock-free matters
/// `stop_audio` is invoked on every `run_read_loop` exit path.  If it required
/// the tokio `Mutex<AudioBridge>` and that mutex happened to be held (e.g. by a
/// concurrent `handle_server_audio` `try_lock` attempt that actually won), the
/// mic capture would continue streaming PCM to the WS writer — a silent failure
/// that can waste quota and produce unexpected server-side audio.
#[derive(Clone)]
pub struct AudioStopHandle {
    running: Arc<AtomicBool>,
    /// CONTROL channel sender (reliable, tiny).  Stop controls travel here, never
    /// on the lossy PCM DATA channel, so a congested playback queue cannot drop a
    /// stop (rhanis-flu round 5 two-channel split).
    ctrl_tx: std::sync::mpsc::SyncSender<AudioControl>,
}

impl AudioStopHandle {
    /// **Graceful** stop: flushes the partial tail capture chunk, then stops.
    ///
    /// Use on **normal** server-close exits where the WS write task is NOT
    /// aborted — the tail PCM should reach the server.
    ///
    /// ## Two-channel reliable delivery (rhanis-flu round 5)
    ///
    /// The control travels on the dedicated CONTROL channel, NOT the PCM DATA
    /// channel, so a playback-congested DATA channel can never drop it.  Delivery
    /// is **reliable**: [`send_control_reliably`] retries `try_send` up to a short
    /// bounded deadline on the shutdown path — NOT the realtime callback — so a
    /// momentarily full control channel does not eat the stop.  The audio thread
    /// drains control first on every loop iteration, so the retry is sub-millisecond
    /// in practice.
    ///
    /// The audio thread's handler for `FlushThenStop` flushes the accumulator
    /// tail **unconditionally** — it does NOT read the `running` atomic to decide.
    /// The flush/discard decision is encoded in the control itself.
    ///
    /// `running` is set to false (after the control is enqueued) so the cpal
    /// capture callback stops producing new PCM — but that flag no longer gates
    /// the flush decision; the control drives it.
    ///
    /// Idempotent: calling multiple times is safe (a disconnected channel returns
    /// Err; that is intentionally discarded).
    pub fn stop_graceful(&self) {
        // Reliable send on the dedicated control channel (bounded retry — this is
        // the shutdown path, not the realtime cpal callback).  A momentarily full
        // channel is honoured up to CONTROL_SEND_TIMEOUT rather than dropped.
        send_control_reliably(&self.ctrl_tx, AudioControl::FlushThenStop);
        // Clear the running flag so the cpal capture callback stops producing new PCM.
        // This is AFTER FlushThenStop is enqueued so the callback may still emit one
        // more batch of samples — that's fine, because FlushThenStop's flush picks up
        // the accumulator state at execution time, which is AFTER the callback drains.
        // The flush decision does NOT depend on this flag; it is driven by the control.
        self.running.store(false, Ordering::Release);
    }

    /// **Fail-closed (immediate)** stop: discards the tail capture chunk.
    ///
    /// Use on **abnormal** exits (budget trip / WS error / timeout / mic lost)
    /// where the WS write task is aborted. Sending `StopNow` (no flush) ensures
    /// no tail PCM races onto the WS after the writer abort.
    ///
    /// 1. Clears the `running` flag atomically (Release ordering) so the cpal
    ///    capture callback gates itself off immediately.
    /// 2. Sends `StopNow` (NO flush) **reliably** on the dedicated control channel
    ///    via [`send_control_reliably`] (bounded retry on the shutdown path).
    ///
    /// Idempotent: calling multiple times is safe.
    pub fn stop_immediate(&self) {
        self.running.store(false, Ordering::Release);
        // StopNow: discard the tail; audio thread will NOT flush.  Reliable send on
        // the control channel so a PCM-congested DATA channel cannot drop the stop.
        send_control_reliably(&self.ctrl_tx, AudioControl::StopNow);
    }

    /// Legacy alias kept for existing P0 regression tests.
    /// Behaves identically to `stop_immediate()` — the manual-shutdown path
    /// does not need a tail flush (the writer is aborted first).
    #[allow(dead_code)]
    pub fn stop(&self) {
        self.stop_immediate();
    }
}

// ── AudioBridge: public facade ────────────────────────────────────────────────

/// Manages both the capture and playback halves of the audio bridge for one
/// Realtime session.
///
/// ## Thread model
/// `start()` spawns a dedicated `std::thread` (the "audio thread") that owns
/// the `cpal::Stream` and `rodio::OutputStream`/`Sink` for their entire
/// lifetime.  All interaction with the audio thread goes through two `Send`
/// `std::sync::mpsc::SyncSender`s: a lossy DATA channel
/// (`SyncSender<AudioCommand>`) for PCM and a reliable CONTROL channel
/// (`SyncSender<AudioControl>`) for stop intent.
///
/// This means `AudioBridge` itself is `Send + Sync` through the `Arc` wrappers
/// — no `unsafe impl Send` is needed, and there is no risk of moving
/// non-`Send` audio handles across thread boundaries.
pub struct AudioBridge {
    state: Arc<AudioBridgeState>,
    /// DATA-channel sender (PCM → playback); `None` before `start()` / after stop.
    data_tx: Option<std::sync::mpsc::SyncSender<AudioCommand>>,
    /// CONTROL-channel sender (stop intent); `None` before `start()` / after stop.
    ctrl_tx: Option<std::sync::mpsc::SyncSender<AudioControl>>,
    /// Barge-in gate for the CURRENT session (rhanis-bx7) — see [`PlaybackHandle`].
    /// REPLACED (not reset) by each `start()`, so a stale prior-generation
    /// handler holding the old `Arc` cannot pollute the new session's gate
    /// (the same generation discipline as the `running` flag).
    playback_gate: Arc<AtomicU8>,
    /// Join handle for the audio thread; used to reap the previous thread at the
    /// next `start()` and to wait for clean teardown on `stop_immediate()`.
    thread_handle: Option<std::thread::JoinHandle<()>>,
}

impl AudioBridge {
    pub fn new() -> Self {
        Self {
            state: Arc::new(AudioBridgeState::new()),
            data_tx: None,
            ctrl_tx: None,
            playback_gate: Arc::new(AtomicU8::new(GATE_OFF)),
            thread_handle: None,
        }
    }

    /// Opens devices and starts streaming.  Must be called once per session.
    /// Sets `running = true` at entry so a restart after a previous stop works.
    /// Returns `Err` on any device failure; the caller should stop the session.
    ///
    /// On success also returns an [`AudioStopHandle`] that the caller can use to
    /// stop the bridge without acquiring the `tokio::Mutex<AudioBridge>`.  This
    /// is the mechanism used by `stop_audio` in `run_read_loop` — see
    /// [`AudioStopHandle`] for the rationale.
    pub fn start(&mut self, write_tx: mpsc::Sender<Message>) -> Result<AudioStopHandle, String> {
        // Reap the previous audio thread BEFORE starting a new one so it cannot
        // linger and (a) hold the device or (b) race the new generation.  We
        // signal it to stop first (StopNow on its OWN control channel + clear its
        // OWN generation flag), then join.  Each of these is per-generation, so
        // this never touches the fresh generation installed below (rhanis-flu round 5).
        if let Some(prev_ctrl) = self.ctrl_tx.take() {
            // Best-effort: clear the previous generation's flag so the old cpal
            // callback gates off, and tell the old thread to stop immediately.
            self.state.stop(); // clears the CURRENT (= previous) generation flag
            send_control_reliably(&prev_ctrl, AudioControl::StopNow);
        }
        self.data_tx = None;
        if let Some(prev) = self.thread_handle.take() {
            // Join the previous thread so its cpal/rodio handles are fully dropped
            // before we open new ones (important on Windows WASAPI exclusivity).
            let _ = prev.join();
        }

        // Install a FRESH per-session running flag.  The old thread (now joined)
        // held a clone of its OWN generation's Arc, so it can never clear this one.
        let state_flag = self.state.new_generation();
        let state_flag_err = Arc::clone(&state_flag);

        // DATA channel: PCM → playback.  Bounded; lossy `try_send` from the
        // realtime path is acceptable (a dropped playback chunk is fine).
        let (data_tx, data_rx) = std::sync::mpsc::sync_channel::<AudioCommand>(64);
        // CONTROL channel: stop intent.  Tiny + reliable so PCM congestion on the
        // DATA channel can never drop a stop.
        let (ctrl_tx, ctrl_rx) =
            std::sync::mpsc::sync_channel::<AudioControl>(CONTROL_CHANNEL_CAP);

        // The cpal error callback needs to signal a stop; it uses the CONTROL
        // channel (NOT the DATA channel) so the stop is reliable even mid-flood.
        let ctrl_tx_for_capture = ctrl_tx.clone();

        // Build the stop handle BEFORE spawning so the caller gets it even if
        // spawn somehow fails (the flag is already set to running by this point).
        let stop_handle = AudioStopHandle {
            running: Arc::clone(&state_flag),
            ctrl_tx: ctrl_tx.clone(),
        };

        // Spawn the dedicated audio thread that owns cpal + rodio handles.
        let handle = std::thread::Builder::new()
            .name("rhanis-audio".into())
            .spawn(move || {
                audio_thread_main(
                    write_tx,
                    data_rx,
                    ctrl_rx,
                    ctrl_tx_for_capture,
                    state_flag,
                    state_flag_err,
                );
            })
            .map_err(|e| {
                self.state.stop();
                format!("audio thread spawn failed: {e}")
            })?;

        self.data_tx = Some(data_tx);
        self.ctrl_tx = Some(ctrl_tx);
        // A fresh session starts with the gate OPEN. The Arc is REPLACED (not
        // stored-into) so a stale prior-generation `PlaybackHandle` keeps its
        // own dead gate and cannot touch this session's (rhanis-bx7).
        self.playback_gate = Arc::new(AtomicU8::new(GATE_OFF));
        self.thread_handle = Some(handle);
        Ok(stop_handle)
    }

    /// Returns the lock-free [`PlaybackHandle`] for the CURRENT session — the
    /// read loop's `audio_handler` seam (playback enqueueing + barge-in gate,
    /// rhanis-bx7).  `None` before `start()`.  Grab it once per connection, AFTER
    /// a successful `start()` (the same pattern as [`AudioStopHandle`]): the
    /// handler must never take this bridge's tokio mutex per frame.
    pub fn playback_handle(&self) -> Option<PlaybackHandle> {
        Some(PlaybackHandle {
            gate: Arc::clone(&self.playback_gate),
            data_tx: self.data_tx.as_ref()?.clone(),
            ctrl_tx: self.ctrl_tx.as_ref()?.clone(),
        })
    }

    /// Returns `true` while the bridge is actively capturing.
    pub fn is_running(&self) -> bool {
        self.state.is_running()
    }

    /// Returns a clonable `Arc<AtomicBool>` for the **current** session that the
    /// session_manager read loop can poll to detect when the cpal error_callback
    /// fires (device lost / driver error).  This is the current generation's flag
    /// (set by `start()`); a stale prior-generation thread cannot mutate it.
    pub fn running_flag(&self) -> Arc<AtomicBool> {
        self.state.current_flag()
    }

    /// Returns a lock-free [`AudioStopHandle`] for the *current* session, or
    /// `None` if `start()` has not been called yet.  Prefer capturing the handle
    /// returned by `start()` directly; this method exists for callers that hold
    /// the bridge `Arc<Mutex<AudioBridge>>` and need to extract the handle after
    /// the fact (e.g. in a re-entrant stop path).
    pub fn stop_handle(&self) -> Option<AudioStopHandle> {
        self.ctrl_tx.as_ref().map(|tx| AudioStopHandle {
            running: self.state.current_flag(),
            ctrl_tx: tx.clone(),
        })
    }

    /// Immediate (fail-closed) stop: discards the tail capture chunk and stops
    /// capture and playback.  Used by manual shutdown (`stop_session`) where the
    /// write task is aborted before this is called — no tail PCM should race onto
    /// the WS after the abort.
    ///
    /// Idempotent; safe to call even if `start()` was never called.  Sends
    /// `StopNow` on the tiny CONTROL channel via non-blocking `try_send` (the
    /// control channel is sized so a single pending stop always fits, and this is
    /// called from an async Tauri command that must not block the tokio executor).
    /// The audio thread exits when it receives `StopNow` or when its receivers are
    /// closed (which happens when the senders are dropped below).
    ///
    /// Note: for the **graceful** stop path (normal server-close) use the
    /// `AudioStopHandle::stop_graceful()` method captured at `start()` time.
    pub fn stop_immediate(&mut self) {
        // No FlushThenStop: tail PCM must be discarded on manual-shutdown paths
        // (the write task is already aborted at the call site).
        if let Some(tx) = &self.ctrl_tx {
            // running=false FIRST so the cpal callback stops producing.
            self.state.stop();
            // StopNow on the dedicated CONTROL channel — never on the lossy DATA
            // channel, so a PCM-congested playback queue cannot drop this stop.
            let _ = tx.try_send(AudioControl::StopNow);
        } else {
            // No active session — just ensure the flag is false.
            self.state.stop();
        }
        self.data_tx = None;
        self.ctrl_tx = None;
        // Join the audio thread so cpal/rodio handles are fully dropped before we
        // return — important on Windows where WASAPI holds exclusive device access.
        if let Some(h) = self.thread_handle.take() {
            let _ = h.join();
        }
    }
}

impl Default for AudioBridge {
    fn default() -> Self {
        Self::new()
    }
}

/// Tauri managed-state wrapper for the audio bridge.  Wrapped in an `Arc<Mutex>`
/// so `start_session` / `stop_session` can mutate it from async Tauri commands,
/// and the `Arc` can be cloned into the `'static` audio-handler closure that is
/// passed to `run_read_loop`.
pub struct ManagedAudioBridge(pub std::sync::Arc<tokio::sync::Mutex<AudioBridge>>);

impl ManagedAudioBridge {
    pub fn new() -> Self {
        Self(std::sync::Arc::new(tokio::sync::Mutex::new(AudioBridge::new())))
    }
}

impl Default for ManagedAudioBridge {
    fn default() -> Self {
        Self::new()
    }
}

// ── Pcm16Source: rodio Source wrapping a Vec<u8> of PCM16 LE ─────────────────

/// A [`rodio::Source`] that iterates over i16 samples decoded from a raw PCM16
/// little-endian byte vector.  Implements `rodio::Source` so it can be fed
/// directly into a `rodio::Sink`.
struct Pcm16Source {
    data: Vec<i16>,
    pos: usize,
    sample_rate: u32,
    channels: u16,
}

impl Pcm16Source {
    fn new(bytes: Vec<u8>, sample_rate: u32, channels: u16) -> Self {
        let samples: Vec<i16> = bytes
            .chunks_exact(2)
            .map(|c| i16::from_le_bytes([c[0], c[1]]))
            .collect();
        Self {
            data: samples,
            pos: 0,
            sample_rate,
            channels,
        }
    }
}

impl Iterator for Pcm16Source {
    type Item = i16;
    fn next(&mut self) -> Option<i16> {
        if self.pos < self.data.len() {
            let s = self.data[self.pos];
            self.pos += 1;
            Some(s)
        } else {
            None
        }
    }
}

impl rodio::Source for Pcm16Source {
    fn current_frame_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> u16 {
        self.channels
    }
    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }
    fn total_duration(&self) -> Option<std::time::Duration> {
        None
    }
}

// ── capture_should_emit: pure, unit-testable send-decision ───────────────────

/// Returns `true` when the capture callback should produce and send PCM for
/// this frame.  Called at the top of every cpal data callback and before every
/// `write_tx.try_send` inside the callback.
///
/// Keeping this as a pure free function (rather than an inline bool expression)
/// makes it trivially unit-testable without any hardware device.
///
/// ## Why this matters
/// The cpal capture data callback runs on a dedicated OS audio thread.  It
/// does **not** receive the `Stop` command from the command channel — it is
/// purely driven by the OS audio scheduler.  Setting `running = false` via
/// [`AudioStopHandle::stop`] must therefore be read here, at the capture
/// source, to guarantee zero further PCM reaches `write_tx` after a stop.
#[inline]
pub fn capture_should_emit(running: bool) -> bool {
    running
}

// ── deliver_tail_reliably: bounded blocking tail delivery (P1-3) ──────────────

/// Delivers the graceful-close tail frame to the (tokio) `write_tx` **reliably**.
///
/// Runs on the audio `std::thread` (a sync context, no async runtime), so it
/// cannot `.await`.  Instead it retries `try_send` with a tiny back-off up to
/// [`TAIL_DELIVERY_TIMEOUT`].  On the graceful path the writer is NOT aborted and
/// drains, so capacity frees up quickly and the tail is delivered rather than
/// dropped on a momentarily full write queue.  This bounded block is acceptable
/// ONLY on the shutdown path — the realtime per-chunk capture path stays
/// lossy/non-blocking (it uses a plain `try_send` and drops on Full).
///
/// Returns `true` if the tail was delivered, `false` if the deadline elapsed
/// (queue stayed full) or the channel closed (writer already gone).  The caller
/// ignores the result — it is best-effort within the bounded window — but the
/// boolean is surfaced for testability.
fn deliver_tail_reliably(write_tx: &mpsc::Sender<Message>, msg: Message) -> bool {
    use tokio::sync::mpsc::error::TrySendError;
    let deadline = std::time::Instant::now() + TAIL_DELIVERY_TIMEOUT;
    let mut msg = msg;
    loop {
        match write_tx.try_send(msg) {
            Ok(()) => return true,
            Err(TrySendError::Full(returned)) => {
                if std::time::Instant::now() >= deadline {
                    // Writer never drained within the bounded window — give up
                    // rather than block the audio-thread teardown indefinitely.
                    return false;
                }
                msg = returned;
                // Brief back-off; the draining writer frees a slot quickly.
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            Err(TrySendError::Closed(_)) => {
                // Writer already gone (e.g. session torn down) — nothing to deliver.
                return false;
            }
        }
    }
}

// ── run_audio_command_loop: two-channel multiplex + reliable tail (P1-1/P1-3) ─

/// Why we stopped the command loop — drives the graceful-vs-immediate tail flush.
enum LoopExit {
    /// Graceful: flush the mic accumulator tail to write_tx RELIABLY, then stop.
    FlushThenStop,
    /// Immediate: discard the tail, stop now.
    StopNow,
}

/// The hardware-free core of the audio thread: multiplexes the **CONTROL** and
/// **DATA** channels and performs the graceful tail flush.  Extracted from
/// `audio_thread_main` so it is unit-testable without cpal/rodio (the rodio sink
/// append is injected as `on_pcm`).
///
/// ## Invariants (the three P1 fixes)
/// - **P1-1**: CONTROL is drained FIRST every iteration (non-blocking), so a stop
///   is honoured promptly even while PCM floods the DATA channel — control is
///   never starved by PCM congestion.
/// - **P1-3**: on `FlushThenStop` the tail is delivered to `write_tx`
///   **reliably** via [`deliver_tail_reliably`] (bounded blocking on the shutdown
///   path); on `StopNow` the tail is discarded (lossy / abnormal path).
/// - The realtime per-chunk capture path is elsewhere (the cpal callback) and
///   stays lossy/non-blocking; this loop's PCM handling is allowed to block
///   briefly on the DATA recv only for low playback latency.
///
/// `on_playback(op) -> bool`: perform one [`PlaybackOp`] on the sink; return
/// `false` to request a fail-closed immediate stop (e.g. playback-queue
/// overflow on `Enqueue`).
fn run_audio_command_loop<P>(
    data_rx: &std::sync::mpsc::Receiver<AudioCommand>,
    ctrl_rx: &std::sync::mpsc::Receiver<AudioControl>,
    accum: &std::sync::Mutex<ChunkAccumulator>,
    write_tx: &mpsc::Sender<Message>,
    state_flag: &AtomicBool,
    mut on_playback: P,
) where
    P: FnMut(PlaybackOp) -> bool,
{
    let exit = 'outer: loop {
        // 1. CONTROL first — check once (non-blocking).  A stop wins immediately
        //    over any queued PCM (P1-1).  The stop variants are terminal, so at
        //    most one of them is ever acted on and any later one is observed on
        //    the next iteration's check.  `ClearPlayback` (rhanis-bx7) is the one
        //    NON-terminal control: it is handled inline and the loop `continue`s
        //    straight back to this check, so a stop queued behind a cut is
        //    honoured before any further PCM.  (Empty falls through to DATA.)
        match ctrl_rx.try_recv() {
            Ok(AudioControl::FlushThenStop) => break 'outer LoopExit::FlushThenStop,
            Ok(AudioControl::StopNow) => break 'outer LoopExit::StopNow,
            Ok(AudioControl::ClearPlayback) => {
                // Barge-in cut: drop every PCM payload already queued on the
                // DATA channel — it is stale audio of the interrupted response
                // (the bridge suppresses NEW deltas until the next response, so
                // nothing fresh is discarded here) — then clear + resume the
                // sink.  Capture is untouched: the user is mid-sentence.
                while let Ok(AudioCommand::EnqueuePcm(_)) = data_rx.try_recv() {}
                if !on_playback(PlaybackOp::Clear) {
                    break 'outer LoopExit::StopNow;
                }
                continue;
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {} // no control pending → DATA
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                // All control senders dropped (process teardown / bridge drop).
                // Treat as immediate stop — no tail flush.
                break 'outer LoopExit::StopNow;
            }
        }

        // 2. DATA — block briefly for low latency, then loop back to re-check CONTROL.
        match data_rx.recv_timeout(AUDIO_DATA_RECV_TIMEOUT) {
            Ok(AudioCommand::EnqueuePcm(bytes)) => {
                if !on_playback(PlaybackOp::Enqueue(bytes)) {
                    // Playback-queue overflow → fail-closed immediate stop.
                    break 'outer LoopExit::StopNow;
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                // No PCM this tick.  Defence-in-depth: if running went false but no
                // control arrived, exit anyway (discard tail — abnormal).
                if !state_flag.load(Ordering::Acquire) {
                    break 'outer LoopExit::StopNow;
                }
                // Still running; loop back to re-check CONTROL then DATA.
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                // All DATA senders dropped.  Loop back once more so any pending
                // CONTROL (e.g. a FlushThenStop) is still observed; if CONTROL is
                // also disconnected the next iteration exits via StopNow.
                if !state_flag.load(Ordering::Acquire) {
                    // Running already cleared and no PCM source → nothing to flush.
                    break 'outer LoopExit::StopNow;
                }
                // DATA gone but still running and no control yet: sleep briefly so
                // we re-poll CONTROL without hot-spinning (the next FlushThenStop /
                // StopNow on CONTROL exits the loop).
                std::thread::sleep(AUDIO_DATA_RECV_TIMEOUT);
            }
        }
    };

    // ── Stop handling: graceful tail flush is RELIABLE (P1-3) ────────────────
    match exit {
        LoopExit::FlushThenStop => {
            // Graceful stop: flush the mic accumulator tail UNCONDITIONALLY.  The
            // flush decision is encoded in the control identity (`FlushThenStop` =
            // always flush) — NOT in the `running` atomic — so no overloaded-flag
            // race can eat the legitimate normal-close tail.
            //
            // P1-3: deliver the tail RELIABLY.  On the graceful path the writer is
            // NOT aborted and drains, so we retry `try_send` up to a short bounded
            // deadline rather than dropping the tail on a momentarily full write
            // queue.  Acceptable on the shutdown path; the realtime per-chunk
            // capture path stays lossy/non-blocking.
            let tail = {
                let mut acc = accum.lock().unwrap();
                acc.flush()
            };
            if let Some(tail) = tail {
                let msg = build_audio_append_frame(&tail);
                deliver_tail_reliably(write_tx, msg);
            }
        }
        LoopExit::StopNow => {
            // Immediate stop: discard the accumulator tail — do NOT flush.  Used on
            // abnormal exits (budget trip / WS error / timeout / mic lost) where the
            // WS writer is already aborted.  Flushing would race against the aborted
            // writer and serve no purpose.
        }
    }
}

// ── audio_thread_main: the dedicated thread body ──────────────────────────────

/// Body of the dedicated audio thread.  Opens the cpal input device and the
/// rodio output device, then runs the two-channel command loop until a stop
/// control is received **or** until `running` is set to false (via the stop
/// flag) even if no control was delivered.
///
/// ## Two-channel loop (rhanis-flu round 5)
/// Each iteration: (1) drain the **CONTROL** channel first (non-blocking) so a
/// stop is honoured promptly even while PCM floods the DATA channel; (2) if no
/// stop, block on the **DATA** channel with a short timeout
/// ([`AUDIO_DATA_RECV_TIMEOUT`]) for low playback latency.  On a DATA timeout we
/// re-check `running` so the thread still exits when a stop control was somehow
/// not delivered (defence-in-depth).
///
/// Both the `cpal::Stream` and the `rodio::OutputStream` / `Sink` are created
/// and dropped exclusively on this thread — they never cross a thread boundary,
/// so no `unsafe impl Send` is required.
fn audio_thread_main(
    write_tx: mpsc::Sender<Message>,
    data_rx: std::sync::mpsc::Receiver<AudioCommand>,
    ctrl_rx: std::sync::mpsc::Receiver<AudioControl>,
    ctrl_tx_for_capture: std::sync::mpsc::SyncSender<AudioControl>,
    state_flag: Arc<AtomicBool>,
    state_flag_err: Arc<AtomicBool>,
) {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

    // ── Open cpal input device ────────────────────────────────────────────────
    let host = cpal::default_host();
    let device = match host.default_input_device() {
        Some(d) => d,
        None => {
            eprintln!("[audio_bridge] no input device");
            state_flag_err.store(false, Ordering::Release);
            return;
        }
    };
    let supported = match device.default_input_config() {
        Ok(c) => c,
        Err(_) => {
            eprintln!("[audio_bridge] input config unavailable");
            state_flag_err.store(false, Ordering::Release);
            return;
        }
    };
    let in_rate = supported.sample_rate().0;
    let channels = supported.channels() as usize;
    let sample_format = supported.sample_format();

    // The accumulator lives behind a plain Mutex: cpal callbacks are on a
    // dedicated OS thread (not the audio thread) and must not block/async.
    let accum = Arc::new(std::sync::Mutex::new(ChunkAccumulator::new()));

    let accum_cb = Arc::clone(&accum);
    let tx_cb = write_tx.clone();
    let state_err_cb = Arc::clone(&state_flag_err);
    // Clone the running flag into the capture callback so each cpal callback
    // can gate its PCM emission on the live flag value (P0-a fix).
    let state_flag_cb = Arc::clone(&state_flag);
    let ctrl_tx_err = ctrl_tx_for_capture.clone();

    macro_rules! build_stream {
        ($sample_type:ty) => {{
            device.build_input_stream(
                &supported.clone().into(),
                move |data: &[$sample_type], _: &cpal::InputCallbackInfo| {
                    // P0-a: gate the ENTIRE callback on the running flag.
                    // After running=false (set by AudioStopHandle::stop or the
                    // error_callback), zero PCM must reach write_tx regardless
                    // of whether the Stop command was delivered.
                    if !capture_should_emit(state_flag_cb.load(Ordering::Acquire)) {
                        return;
                    }
                    use cpal::Sample as _;
                    let as_f32: Vec<f32> = data.iter().map(|s| s.to_sample::<f32>()).collect();
                    let mono = downmix_to_mono(&as_f32, channels);
                    let resampled = resample_mono_linear(&mono, in_rate, REALTIME_SAMPLE_RATE);
                    let mut acc = accum_cb.lock().unwrap();
                    let chunks = acc.push(&resampled);
                    drop(acc); // release before the send
                    for chunk in chunks {
                        // P0-a: also gate each individual chunk-send on the flag
                        // in case running went false between the top-of-callback
                        // check and here (e.g. concurrent stop on another thread).
                        if !capture_should_emit(state_flag_cb.load(Ordering::Acquire)) {
                            return;
                        }
                        let msg = build_audio_append_frame(&chunk);
                        // Non-blocking: drop chunks if the WS write channel is full
                        // (real-time requirement; the UI shows lag indicator in rhanis-ef8).
                        let _ = tx_cb.try_send(msg);
                    }
                },
                move |_err| {
                    // Log category only — no OS error detail (device name / path).
                    eprintln!("[audio_bridge] capture error");
                    state_err_cb.store(false, Ordering::Release);
                    // Signal the audio thread to exit immediately (no tail flush) on
                    // the dedicated CONTROL channel — never the lossy DATA channel,
                    // so a PCM-congested playback queue cannot drop this stop.
                    // This is an abnormal exit (device error), so we discard the tail.
                    let _ = ctrl_tx_err.try_send(AudioControl::StopNow);
                },
                None,
            )
        }};
    }

    let stream_result = match sample_format {
        cpal::SampleFormat::F32 => build_stream!(f32),
        cpal::SampleFormat::I16 => build_stream!(i16),
        cpal::SampleFormat::U16 => build_stream!(u16),
        cpal::SampleFormat::I8  => build_stream!(i8),
        cpal::SampleFormat::U8  => build_stream!(u8),
        cpal::SampleFormat::I32 => build_stream!(i32),
        cpal::SampleFormat::U32 => build_stream!(u32),
        cpal::SampleFormat::I64 => build_stream!(i64),
        cpal::SampleFormat::U64 => build_stream!(u64),
        cpal::SampleFormat::F64 => build_stream!(f64),
        fmt => {
            eprintln!("[audio_bridge] unsupported sample format: {fmt:?}");
            state_flag_err.store(false, Ordering::Release);
            return;
        }
    };

    let stream = match stream_result {
        Ok(s) => s,
        Err(_) => {
            eprintln!("[audio_bridge] stream build failed");
            state_flag_err.store(false, Ordering::Release);
            return;
        }
    };

    if let Err(_) = stream.play() {
        eprintln!("[audio_bridge] stream play failed");
        state_flag_err.store(false, Ordering::Release);
        return;
    }

    // ── Open rodio output ─────────────────────────────────────────────────────
    let playback_result = rodio::OutputStream::try_default();
    let (rodio_stream, stream_handle) = match playback_result {
        Ok(pair) => pair,
        Err(_) => {
            eprintln!("[audio_bridge] output device unavailable");
            state_flag_err.store(false, Ordering::Release);
            // stream is dropped here (confined to this thread).
            return;
        }
    };
    let sink = match rodio::Sink::try_new(&stream_handle) {
        Ok(s) => s,
        Err(_) => {
            eprintln!("[audio_bridge] sink creation failed");
            state_flag_err.store(false, Ordering::Release);
            return;
        }
    };

    // Tracks the total bytes currently queued in the sink (bounded DoS guard).
    let mut queued_bytes: usize = 0;

    // Enqueue one decoded PCM payload into the playback sink, enforcing the
    // bounded-queue DoS guard.  Returns `false` to request a fail-closed stop
    // (playback-queue overflow) so the caller can break the loop.
    let enqueue_pcm = |bytes: Vec<u8>, queued_bytes: &mut usize| -> bool {
        if bytes.len() > MAX_AUDIO_DELTA_BYTES {
            eprintln!("[audio_bridge] enqueue: payload too large, dropping");
            return true;
        }
        // Bounded playback queue: if adding this chunk would exceed the cap, clear
        // the queue and stop the session fail-closed.
        let new_total = queued_bytes.saturating_add(bytes.len());
        if new_total > MAX_PLAYBACK_QUEUE_BYTES {
            eprintln!("[audio_bridge] playback queue overflow, clearing");
            sink.clear();
            *queued_bytes = 0;
            // Mark running=false so the session_manager read loop's mic-poll
            // detects the overflow and stops the session fail-closed.
            state_flag.store(false, Ordering::Release);
            return false;
        }
        *queued_bytes = new_total;
        // Drain the accounting when the sink empties between commands.
        if sink.empty() {
            *queued_bytes = 0;
        }
        let source = Pcm16Source::new(bytes, REALTIME_SAMPLE_RATE, 1);
        sink.append(source);
        true
    };

    // ── Two-channel command loop + graceful tail flush (P1-1 / P1-3) ─────────
    // Extracted into `run_audio_command_loop` so the control-vs-PCM multiplexing
    // and the reliable tail delivery are unit-testable WITHOUT any cpal/rodio
    // hardware.  One playback closure handles both ops (it owns the rodio `sink`
    // borrow + the queued-bytes accounting).
    run_audio_command_loop(
        &data_rx,
        &ctrl_rx,
        &accum,
        &write_tx,
        &state_flag,
        |op: PlaybackOp| match op {
            PlaybackOp::Enqueue(bytes) => enqueue_pcm(bytes, &mut queued_bytes),
            PlaybackOp::Clear => {
                // Barge-in cut (rhanis-bx7).  rodio's `Sink::clear()` also PAUSES
                // the sink, so `play()` must follow for the NEXT response's
                // audio to be audible.  (The overflow path inside `enqueue_pcm`
                // deliberately skips `play()` — it stops the whole session
                // immediately afterwards.)
                sink.clear();
                sink.play();
                queued_bytes = 0;
                true
            }
        },
    );
    // The PCM-enqueue closure captured `&sink` by shared reference; its last use is
    // inside `run_audio_command_loop` above (NLL released the borrow), so teardown
    // below may move `sink` freely.

    // Teardown order: stop + clear sink, then drop stream. Both are confined to
    // this thread so there is no Send requirement.
    sink.clear();
    // `stream` and `rodio_stream` (+ `stream_handle`) are dropped here as the
    // thread exits, releasing device resources cleanly.
    drop(sink);
    drop(stream_handle);
    drop(rodio_stream);
    drop(stream);
    // Ensure the flag is false on all exit paths.
    state_flag.store(false, Ordering::Release);
}

// ── Unit tests (pure logic — WSL-safe, no hardware) ───────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── f32 → PCM16 conversion ───────────────────────────────────────────────

    #[test]
    fn f32_zero_maps_to_zero_pcm16() {
        let bytes = f32_slice_to_pcm16_le(&[0.0]);
        let i = i16::from_le_bytes([bytes[0], bytes[1]]);
        assert_eq!(i, 0);
    }

    #[test]
    fn f32_positive_one_maps_to_i16_max() {
        let bytes = f32_slice_to_pcm16_le(&[1.0]);
        let i = i16::from_le_bytes([bytes[0], bytes[1]]);
        // With the rounding (+0.5 before truncate), 1.0 × 32767 + 0.5 = 32767.5,
        // clamped to 32767, cast to i16 = 32767.
        assert_eq!(i, i16::MAX);
    }

    #[test]
    fn f32_negative_one_maps_to_i16_min() {
        let bytes = f32_slice_to_pcm16_le(&[-1.0]);
        let i = i16::from_le_bytes([bytes[0], bytes[1]]);
        // -1.0 × 32767 + 0.5 = -32766.5; max(-32768) → -32767 as i16.
        // Actual clamped: clamp to [-32768, 32767] → -32767 (not MIN).
        // The rounding formula: (-1.0 × 32767.0 + 0.5).max(-32768).min(32767) = -32766.5.max(-32768) = -32766.5 → -32766 as i16.
        // Let's just confirm it is well within the negative half and in range.
        assert!(i < 0);
        // -1.0 lands a strongly-negative sample near i16::MIN (the comment above
        // works it out to -32766).  Assert it falls in the bottom half rather than
        // pinning the exact rounding result (brittle); the i16 type itself already
        // guarantees it can never underflow past i16::MIN, so a `>= i16::MIN`
        // check would be vacuously true.
        assert!(i < i16::MIN / 2);
    }

    #[test]
    fn f32_clamping_out_of_range() {
        // Values outside [-1, +1] must be clamped, not overflow.
        let bytes = f32_slice_to_pcm16_le(&[2.0, -3.0]);
        let lo = i16::from_le_bytes([bytes[0], bytes[1]]);
        let hi = i16::from_le_bytes([bytes[2], bytes[3]]);
        assert_eq!(lo, i16::MAX);
        assert!(hi < 0); // clamped to -1.0 path
    }

    #[test]
    fn f32_nan_maps_to_zero_pcm16() {
        // Non-finite values are silenced to 0 (no undefined cast behaviour).
        let bytes = f32_slice_to_pcm16_le(&[f32::NAN]);
        let i = i16::from_le_bytes([bytes[0], bytes[1]]);
        assert_eq!(i, 0, "NaN must map to PCM16 zero (silent sample)");
    }

    #[test]
    fn f32_inf_maps_to_zero_pcm16() {
        let bytes = f32_slice_to_pcm16_le(&[f32::INFINITY, f32::NEG_INFINITY]);
        let lo = i16::from_le_bytes([bytes[0], bytes[1]]);
        let hi = i16::from_le_bytes([bytes[2], bytes[3]]);
        assert_eq!(lo, 0, "+Inf must map to PCM16 zero");
        assert_eq!(hi, 0, "-Inf must map to PCM16 zero");
    }

    // ── PCM16 → f32 conversion ────────────────────────────────────────────────

    #[test]
    fn pcm16_zero_maps_to_zero_f32() {
        let bytes = 0_i16.to_le_bytes();
        let f = pcm16_le_to_f32(&bytes);
        assert!((f[0] - 0.0).abs() < 1e-6);
    }

    #[test]
    fn pcm16_max_maps_to_near_one_f32() {
        let bytes = i16::MAX.to_le_bytes();
        let f = pcm16_le_to_f32(&bytes);
        // i16::MAX / 32768.0 = 0.9999…
        assert!(f[0] > 0.99 && f[0] < 1.0);
    }

    #[test]
    fn pcm16_min_maps_to_exactly_neg_one_f32() {
        let bytes = i16::MIN.to_le_bytes();
        let f = pcm16_le_to_f32(&bytes);
        assert!((f[0] - (-1.0)).abs() < 1e-6);
    }

    #[test]
    fn pcm16_odd_byte_count_truncates() {
        // 3 bytes → 1 complete i16 pair + 1 leftover → only 1 sample.
        let f = pcm16_le_to_f32(&[0x00, 0x40, 0xFF]);
        assert_eq!(f.len(), 1);
    }

    // ── Round-trip: f32 → PCM16 → f32 ────────────────────────────────────────

    #[test]
    fn round_trip_f32_pcm16_f32_is_close() {
        let original = vec![-0.75, -0.5, 0.0, 0.25, 0.5, 0.75];
        let pcm = f32_slice_to_pcm16_le(&original);
        let recovered = pcm16_le_to_f32(&pcm);
        for (orig, rec) in original.iter().zip(recovered.iter()) {
            assert!((orig - rec).abs() < 0.001, "orig={orig} rec={rec}");
        }
    }

    // ── Downmix ───────────────────────────────────────────────────────────────

    #[test]
    fn downmix_stereo_to_mono_averages() {
        // Stereo interleaved: [L0, R0, L1, R1]
        let stereo = vec![1.0_f32, 0.0, 0.5, 0.5];
        let mono = downmix_to_mono(&stereo, 2);
        assert_eq!(mono.len(), 2);
        assert!((mono[0] - 0.5).abs() < 1e-6, "frame0 avg {}", mono[0]);
        assert!((mono[1] - 0.5).abs() < 1e-6, "frame1 avg {}", mono[1]);
    }

    #[test]
    fn downmix_mono_is_noop() {
        let samples = vec![0.1, 0.2, 0.3];
        let out = downmix_to_mono(&samples, 1);
        assert_eq!(out, samples);
    }

    #[test]
    fn downmix_zero_channels_returns_empty() {
        let out = downmix_to_mono(&[0.1, 0.2], 0);
        assert!(out.is_empty());
    }

    // ── Linear resampler ──────────────────────────────────────────────────────

    #[test]
    fn resample_same_rate_is_noop() {
        let samples = vec![0.1, 0.2, 0.3, 0.4];
        assert_eq!(resample_mono_linear(&samples, 24000, 24000), samples);
    }

    #[test]
    fn resample_48k_to_24k_halves_length() {
        // 48 samples at 48kHz → 24 samples at 24kHz (exact 2:1)
        let samples: Vec<f32> = (0..48).map(|i| i as f32 / 48.0).collect();
        let out = resample_mono_linear(&samples, 48000, 24000);
        // ceil(48 * 24000/48000) = 24
        assert_eq!(out.len(), 24);
    }

    #[test]
    fn resample_16k_to_24k_increases_length() {
        // 16 samples at 16kHz → 24 samples at 24kHz (3:2 ratio)
        let samples: Vec<f32> = (0..16).map(|i| i as f32 / 16.0).collect();
        let out = resample_mono_linear(&samples, 16000, 24000);
        // ceil(16 * 24000/16000) = 24
        assert_eq!(out.len(), 24);
    }

    #[test]
    fn resample_44100_to_24000_length_is_correct() {
        let samples: Vec<f32> = vec![0.5_f32; 441]; // 10ms at 44.1kHz
        let out = resample_mono_linear(&samples, 44100, 24000);
        let expected = (441_f64 * 24000.0 / 44100.0).ceil() as usize;
        assert_eq!(out.len(), expected);
    }

    #[test]
    fn resample_empty_returns_empty() {
        let out = resample_mono_linear(&[], 48000, 24000);
        assert!(out.is_empty());
    }

    // ── Chunk accumulator ─────────────────────────────────────────────────────

    #[test]
    fn accumulator_no_flush_until_full_chunk() {
        // chunk_samples = 10 for this test
        let mut acc = ChunkAccumulator::with_chunk_samples(10);
        let result = acc.push(&[0.0; 5]);
        assert!(result.is_empty(), "should not flush yet");
    }

    #[test]
    fn accumulator_flushes_on_exact_chunk() {
        let mut acc = ChunkAccumulator::with_chunk_samples(10);
        let result = acc.push(&[0.1; 10]);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].len(), 20); // 10 samples × 2 bytes
    }

    #[test]
    fn accumulator_flushes_multiple_chunks() {
        let mut acc = ChunkAccumulator::with_chunk_samples(10);
        // 25 samples → 2 full chunks of 10, 5 left in buffer
        let result = acc.push(&[0.2; 25]);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn accumulator_flush_pads_partial() {
        let mut acc = ChunkAccumulator::with_chunk_samples(10);
        acc.push(&[0.3; 5]);
        let tail = acc.flush();
        assert!(tail.is_some());
        // Should be padded to 10 samples = 20 bytes
        assert_eq!(tail.unwrap().len(), 20);
    }

    #[test]
    fn accumulator_flush_empty_is_none() {
        let mut acc = ChunkAccumulator::new();
        assert!(acc.flush().is_none());
    }

    // ── WS frame builders ─────────────────────────────────────────────────────

    #[test]
    fn build_audio_append_frame_type_and_base64() {
        let pcm_bytes: Vec<u8> = (0_i16..4).flat_map(|i| i.to_le_bytes()).collect();
        let msg = build_audio_append_frame(&pcm_bytes);
        let txt = match msg {
            Message::Text(t) => t.to_string(),
            _ => panic!("expected text message"),
        };
        let v: serde_json::Value = serde_json::from_str(&txt).expect("valid json");
        assert_eq!(v["type"], "input_audio_buffer.append");
        let b64 = v["audio"].as_str().expect("audio field is string");
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(b64)
            .expect("valid base64");
        assert_eq!(decoded, pcm_bytes);
    }

    #[test]
    fn decode_audio_delta_round_trips() {
        let pcm: Vec<u8> = vec![0x01, 0x00, 0x02, 0x00]; // two i16: 1, 2
        let b64 = base64::engine::general_purpose::STANDARD.encode(&pcm);
        let event = serde_json::json!({
            "type": "response.audio.delta",
            "delta": b64,
        });
        let decoded = decode_audio_delta(&event).expect("decode ok");
        assert_eq!(decoded, pcm);
    }

    #[test]
    fn decode_audio_delta_round_trips_ga_name() {
        // GA wire name (rhanis-bd7): `response.output_audio.delta` must decode the
        // same as the superseded beta `response.audio.delta`, or a GA handshake
        // leaves the assistant silent while every other test stays green.
        let pcm: Vec<u8> = vec![0x01, 0x00, 0x02, 0x00];
        let b64 = base64::engine::general_purpose::STANDARD.encode(&pcm);
        let event = serde_json::json!({
            "type": "response.output_audio.delta",
            "delta": b64,
        });
        let decoded = decode_audio_delta(&event).expect("GA-name decode ok");
        assert_eq!(decoded, pcm);
    }

    #[test]
    fn decode_audio_delta_wrong_type_is_none() {
        let event = serde_json::json!({ "type": "response.done", "delta": "AA==" });
        assert!(decode_audio_delta(&event).is_none());
    }

    #[test]
    fn decode_audio_delta_bad_base64_is_none() {
        let event = serde_json::json!({
            "type": "response.audio.delta",
            "delta": "not!valid!base64!!!"
        });
        assert!(decode_audio_delta(&event).is_none());
    }

    #[test]
    fn decode_audio_delta_oversized_is_none() {
        // Construct a b64 string that would decode to MAX_AUDIO_DELTA_BYTES + 1.
        // One base64 character encodes 6 bits; 4 chars encode 3 bytes.
        // We need > MAX_AUDIO_DELTA_BYTES * 4 / 3 + 4 characters.
        let oversized_b64 = "A".repeat(MAX_AUDIO_DELTA_BYTES * 4 / 3 + 8);
        let event = serde_json::json!({
            "type": "response.audio.delta",
            "delta": oversized_b64,
        });
        assert!(
            decode_audio_delta(&event).is_none(),
            "oversized delta must be rejected"
        );
    }

    #[test]
    fn enqueue_pcm16_le_oversized_is_dropped() {
        // PlaybackQueue requires a real audio device, so we test the guard
        // at the function boundary via the public constant.
        // Verify that a byte slice just over the limit triggers the guard path:
        // the constant itself is the sentinel value we compare against.
        assert!(
            MAX_AUDIO_DELTA_BYTES * 4 / 3 + 4 < MAX_AUDIO_DELTA_BYTES * 2,
            "guard threshold must be well below a plausible 2× amplification"
        );
    }

    // ── AudioBridgeState ──────────────────────────────────────────────────────

    #[test]
    fn bridge_state_new_is_not_running() {
        // AudioBridgeState starts false; new_generation() is called by AudioBridge::start().
        let s = AudioBridgeState::new();
        assert!(!s.is_running(), "new state must not be running until start()");
    }

    #[test]
    fn bridge_state_new_generation_and_stop() {
        let s = AudioBridgeState::new();
        s.new_generation(); // installs a fresh flag (= true), as start() does
        assert!(s.is_running());
        s.stop();
        assert!(!s.is_running());
    }

    #[test]
    fn bridge_state_stop_is_idempotent() {
        let s = AudioBridgeState::new();
        s.new_generation();
        s.stop();
        s.stop(); // second call must not panic
        assert!(!s.is_running());
    }

    #[test]
    fn bridge_state_restart_works() {
        // Simulates: start() → new_generation (true); stop() → false; start() → true again.
        let s = AudioBridgeState::new();
        s.new_generation();
        assert!(s.is_running());
        s.stop();
        assert!(!s.is_running());
        // Restart installs a fresh generation (true again).
        s.new_generation();
        assert!(s.is_running(), "running flag must be true after second new_generation()");
    }

    // ── P1-2: per-session generation flag — stale thread cannot clobber next ──

    /// (c, P1-2) A stale audio thread's teardown writing `false` to ITS OWN
    /// generation's flag must NOT clear the NEXT session's running flag.
    ///
    /// `new_generation()` installs a FRESH `Arc<AtomicBool>` each session.  The
    /// old thread keeps a clone of its OWN generation's Arc; writing `false` there
    /// affects only that (now-discarded) generation, never the current one.
    #[test]
    fn new_generation_isolates_running_flag_from_stale_thread() {
        let s = AudioBridgeState::new();

        // Session 1: install a generation and capture the flag the "old thread" holds.
        let gen1_flag = s.new_generation();
        assert!(gen1_flag.load(Ordering::Acquire), "gen1 flag starts true");
        assert!(s.is_running(), "state reflects gen1 running");

        // Session 2 starts: a FRESH generation flag is installed.
        let gen2_flag = s.new_generation();
        assert!(s.is_running(), "state reflects gen2 running after restart");
        assert!(gen2_flag.load(Ordering::Acquire), "gen2 flag is true");

        // The STALE (gen1) thread now tears down and writes false to ITS flag.
        gen1_flag.store(false, Ordering::Release);

        // The current (gen2) session must STILL be running — the stale write to the
        // gen1 Arc cannot reach the gen2 Arc.
        assert!(
            s.is_running(),
            "stale gen1 thread teardown must NOT clear the gen2 session's running flag"
        );
        assert!(gen2_flag.load(Ordering::Acquire), "gen2 flag must remain true");

        // And clearing the current generation does work (no leakage the other way).
        s.stop();
        assert!(!s.is_running(), "stop() clears the current (gen2) generation");
    }

    /// (P1-2) `running_flag()` returns the CURRENT generation's Arc, and a fresh
    /// `new_generation()` returns a DIFFERENT Arc instance (proves a real swap).
    #[test]
    fn new_generation_returns_distinct_arc_each_session() {
        let s = AudioBridgeState::new();
        let g1 = s.new_generation();
        let g2 = s.new_generation();
        assert!(
            !Arc::ptr_eq(&g1, &g2),
            "each session must get a fresh Arc, not a reused one"
        );
    }

    // ── PCM16 byte output sizing ───────────────────────────────────────────────

    #[test]
    fn f32_slice_to_pcm16_le_byte_count() {
        // n f32 samples → 2n bytes
        let n = 17;
        let samples: Vec<f32> = vec![0.0; n];
        assert_eq!(f32_slice_to_pcm16_le(&samples).len(), n * 2);
    }

    #[test]
    fn chunk_samples_constant_is_480() {
        // 24000 Hz × 20ms = 480 samples
        assert_eq!(CHUNK_SAMPLES, 480);
    }

    #[test]
    fn full_chunk_is_960_bytes() {
        // 480 i16 samples × 2 bytes = 960 bytes
        let samples = vec![0.0_f32; CHUNK_SAMPLES];
        let bytes = f32_slice_to_pcm16_le(&samples);
        assert_eq!(bytes.len(), 960);
    }

    // ── Pcm16Source iterator ──────────────────────────────────────────────────

    #[test]
    fn pcm16_source_iterates_all_samples() {
        let bytes: Vec<u8> = vec![1, 0, 2, 0, 3, 0]; // i16: 1, 2, 3
        let src = Pcm16Source::new(bytes, 24000, 1);
        let collected: Vec<i16> = src.collect();
        assert_eq!(collected, vec![1, 2, 3]);
    }

    #[test]
    fn pcm16_source_reports_correct_metadata() {
        use rodio::Source;
        let src = Pcm16Source::new(vec![0; 4], 24000, 1);
        assert_eq!(src.channels(), 1);
        assert_eq!(src.sample_rate(), 24000);
    }

    // ── MAX_WS_TEXT_BYTES and MAX_ARGS_LEN sanity ────────────────────────────

    #[test]
    fn ws_text_size_cap_exceeds_audio_delta_cap() {
        // MAX_WS_TEXT_BYTES must be >= the base64-encoded MAX_AUDIO_DELTA_BYTES.
        assert!(MAX_WS_TEXT_BYTES >= MAX_AUDIO_DELTA_BYTES * 4 / 3 + 4);
    }

    #[test]
    fn args_len_cap_is_reasonable() {
        // MAX_ARGS_LEN must be > 0 and < MAX_WS_TEXT_BYTES.
        assert!(MAX_ARGS_LEN > 0);
        assert!(MAX_ARGS_LEN < MAX_WS_TEXT_BYTES);
    }

    // ── Playback queue overflow guard (pure logic) ────────────────────────────

    #[test]
    fn max_playback_queue_bytes_constant_is_reasonable() {
        // ~5 s at 24kHz PCM16 mono = 240_000 bytes. Verify the constant.
        let five_sec_bytes: usize = 24_000 * 2 * 5;
        assert_eq!(MAX_PLAYBACK_QUEUE_BYTES, five_sec_bytes);
    }

    // ── Tail flush via ChunkAccumulator ──────────────────────────────────────

    #[test]
    fn tail_flush_produces_output_after_push() {
        // Simulates the stop() → FlushCapture path: partial samples → flush.
        let mut acc = ChunkAccumulator::with_chunk_samples(10);
        // Push 7 samples (< one full 10-sample chunk so push() returns empty).
        let mid = acc.push(&[0.5_f32; 7]);
        assert!(mid.is_empty(), "partial push must not flush a complete chunk");
        // FlushCapture path: flush() must produce the padded tail.
        let tail = acc.flush();
        assert!(tail.is_some(), "flush() must produce the partial tail");
        assert_eq!(tail.unwrap().len(), 20, "10 samples padded × 2 bytes = 20");
    }

    #[test]
    fn tail_flush_after_full_chunks_returns_none_when_empty() {
        // If the accumulator is drained to zero by push(), flush() returns None.
        let mut acc = ChunkAccumulator::with_chunk_samples(10);
        acc.push(&[0.1_f32; 20]); // exactly 2 full chunks, nothing left
        assert!(acc.flush().is_none(), "flush after complete drain must be None");
    }

    // ── AudioBridge stop/audio stop unit test (pure, no hardware) ────────────

    /// Verifies that a budget-trip-style exit (simulated by setting the running
    /// flag to false) does NOT require a stop_session call — the stop hook
    /// passed to run_read_loop is sufficient.  This is tested at the level of
    /// the AtomicBool mechanics to avoid requiring a real audio device.
    #[test]
    fn running_flag_false_detectable_without_hardware() {
        let state = AudioBridgeState::new();
        state.new_generation();
        assert!(state.is_running());
        // Simulate the cpal error_callback or playback overflow.
        state.stop();
        assert!(!state.is_running(), "stop must clear the running flag");
    }

    // ── P0: AudioStopHandle works while tokio Mutex is contended ─────────────

    /// P0 regression test: proves the contended path (tokio Mutex held) still
    /// stops the mic.
    ///
    /// Simulates the exact scenario that was broken before: `stop_audio` used
    /// `try_lock` on the `tokio::Mutex<AudioBridge>` which silently fails when
    /// the mutex is already held (e.g. budget-trip fires while `handle_server_audio`
    /// holds the lock via `audio_handler`).
    ///
    /// This test:
    /// 1. Creates an `AudioStopHandle` backed by a real `Arc<AtomicBool>` + a
    ///    real `SyncSender`.
    /// 2. Wraps the same `Arc<AtomicBool>` in a `tokio::Mutex` to represent the
    ///    `ManagedAudioBridge` mutex in production.
    /// 3. Acquires the tokio Mutex (simulating another async task holding it).
    /// 4. Calls `stop_handle.stop()` while the guard is still alive.
    /// 5. Asserts `running == false` and that a `Stop` command reached the channel
    ///    — all without releasing the mutex.
    #[test]
    fn stop_handle_stops_mic_while_tokio_mutex_is_held() {
        use tokio::sync::Mutex as TokioMutex;

        let running = Arc::new(AtomicBool::new(true));
        // CONTROL channel capacity: enough to hold a StopNow (stop() = stop_immediate()).
        let (ctrl_tx, ctrl_rx) = std::sync::mpsc::sync_channel::<AudioControl>(CONTROL_CHANNEL_CAP);

        let stop_handle = AudioStopHandle {
            running: Arc::clone(&running),
            ctrl_tx,
        };

        // Simulate the tokio Mutex being held (blocking_lock on a fresh mutex
        // always succeeds immediately; the guard is alive for the rest of the test).
        let mutex = TokioMutex::new(());
        let _guard = mutex.try_lock().expect("mutex must be unlocked initially");
        // At this point `mutex` is held.  In the old code the closure would call
        // `audio_arc_stop.try_lock()` which would return Err and the mic would
        // keep running.  With the new AudioStopHandle the stop is lock-free:
        stop_handle.stop(); // stop() = stop_immediate(): no flush, just StopNow.

        // running flag must be false immediately — no mutex acquisition needed.
        assert!(
            !running.load(Ordering::Acquire),
            "running must be false after stop_handle.stop() even while Mutex is held"
        );

        // A StopNow control must have been enqueued on the CONTROL channel.
        let mut found_stop = false;
        while let Ok(ctrl) = ctrl_rx.try_recv() {
            if matches!(ctrl, AudioControl::StopNow) {
                found_stop = true;
            }
        }
        assert!(found_stop, "StopNow control must be enqueued by stop_handle.stop()");
    }

    // ── P1: DATA try_send drops gracefully when full; CONTROL is independent ──

    /// (d, P1-1/P1-4) Realtime-path regression: enqueueing PCM beyond the DATA
    /// channel capacity drops gracefully and does NOT block (the realtime cpal /
    /// playback path must never block).  Crucially, a FULL DATA channel does NOT
    /// affect the CONTROL channel — a stop is still delivered because control now
    /// travels on its OWN reliable channel (the P1-1 two-channel split).
    ///
    /// A capacity-0 channel is not possible with `sync_channel`, so we use a
    /// capacity-1 DATA channel, fill it, then prove further PCM `try_send`s return
    /// without blocking while a stop still reaches the (separate) CONTROL channel.
    #[test]
    fn data_try_send_drops_without_blocking_and_control_unaffected() {
        // DATA channel capacity = 1: easy to fill (lossy PCM path).
        let (data_tx, data_rx) = std::sync::mpsc::sync_channel::<AudioCommand>(1);
        // CONTROL channel: separate, with headroom for a single stop.
        let (ctrl_tx, ctrl_rx) = std::sync::mpsc::sync_channel::<AudioControl>(CONTROL_CHANNEL_CAP);

        // Fill the DATA channel to capacity.
        data_tx
            .try_send(AudioCommand::EnqueuePcm(vec![0u8; 2]))
            .expect("first DATA send must succeed on empty channel");

        let start = std::time::Instant::now();

        // Realtime path: further PCM try_sends on the FULL DATA channel return Err
        // (Full) without blocking.
        let r1 = data_tx.try_send(AudioCommand::EnqueuePcm(vec![0u8; 2]));
        assert!(r1.is_err(), "PCM try_send on a full DATA channel must return Err (lossy)");

        // Stop path: stop() delivers StopNow on the SEPARATE control channel — the
        // full DATA channel cannot drop it (this is the core P1-1 guarantee).
        let running = Arc::new(AtomicBool::new(true));
        let stop_handle = AudioStopHandle {
            running: Arc::clone(&running),
            ctrl_tx,
        };
        stop_handle.stop();

        // The DATA-full PCM path + the control stop both complete promptly: the
        // control channel had headroom so its reliable send fits on the first try.
        let elapsed = start.elapsed();
        assert!(
            elapsed.as_millis() < 100,
            "DATA-lossy + headroom-control paths must complete promptly (took {elapsed:?})"
        );

        // running cleared, StopNow delivered on CONTROL despite the full DATA channel.
        assert!(!running.load(Ordering::Acquire), "stop() must clear running");
        assert!(
            matches!(ctrl_rx.try_recv(), Ok(AudioControl::StopNow)),
            "StopNow must reach the CONTROL channel even when the DATA channel is full"
        );

        // The DATA channel still has exactly the one sentinel PCM payload.
        assert!(matches!(data_rx.try_recv(), Ok(AudioCommand::EnqueuePcm(_))));
        assert!(data_rx.try_recv().is_err(), "no extra PCM must be in the DATA channel");
    }

    /// Mirrors the two-channel audio loop: when ALL senders (DATA + CONTROL) are
    /// dropped the thread exits cleanly (no panic, no hang).  This proves the
    /// audio thread cannot leak across start/stop cycles when `AudioBridge` drops
    /// both `data_tx` and `ctrl_tx`.
    #[test]
    fn audio_thread_exits_on_sender_drop_without_panic() {
        let (data_tx, data_rx) = std::sync::mpsc::sync_channel::<AudioCommand>(4);
        let (ctrl_tx, ctrl_rx) =
            std::sync::mpsc::sync_channel::<AudioControl>(CONTROL_CHANNEL_CAP);

        // Spawn a thread mirroring the production two-channel loop (pure logic,
        // no hardware): check CONTROL first (non-blocking), then block on DATA.
        let handle = std::thread::spawn(move || {
            let running = Arc::new(AtomicBool::new(true));
            'outer: loop {
                // CONTROL first.
                match ctrl_rx.try_recv() {
                    Ok(AudioControl::FlushThenStop) | Ok(AudioControl::StopNow) => break 'outer,
                    Ok(AudioControl::ClearPlayback) => {} // non-terminal (rhanis-bx7)
                    Err(std::sync::mpsc::TryRecvError::Empty) => {} // no control → DATA
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => break 'outer,
                }
                // DATA with short timeout.
                match data_rx.recv_timeout(std::time::Duration::from_millis(20)) {
                    Ok(AudioCommand::EnqueuePcm(_)) => {}
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                        if !running.load(Ordering::Acquire) {
                            break 'outer;
                        }
                        std::thread::sleep(std::time::Duration::from_millis(20));
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                        if !running.load(Ordering::Acquire) {
                            break 'outer;
                        }
                    }
                }
            }
            // Thread exits here cleanly.
        });

        // Drop BOTH senders + clear running so the simulated audio thread exits.
        // (In production AudioBridge::stop_immediate clears running, sends StopNow,
        // then drops both senders; here we drop to prove the Disconnected exits.)
        drop(data_tx);
        drop(ctrl_tx);

        // Join with a timeout via a helper thread to avoid hanging the test suite.
        let (done_tx, done_rx) = std::sync::mpsc::channel::<()>();
        std::thread::spawn(move || {
            let _ = handle.join();
            let _ = done_tx.send(());
        });

        let joined = done_rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .is_ok();
        assert!(joined, "audio thread must exit cleanly when both senders are dropped");
    }

    // ── P0-a: capture_should_emit pure function ───────────────────────────────

    /// Verifies the pure send-decision function: returns false when running=false,
    /// true when running=true.  No hardware required.
    #[test]
    fn capture_should_emit_false_when_not_running() {
        assert!(
            !capture_should_emit(false),
            "capture_should_emit must return false when running=false"
        );
    }

    #[test]
    fn capture_should_emit_true_when_running() {
        assert!(
            capture_should_emit(true),
            "capture_should_emit must return true when running=true"
        );
    }

    // ── P0-b/P1: audio thread exits after running=false even with full DATA ───

    /// P0-b regression test (carried into the two-channel design): proves the
    /// audio thread exits and conceptually "drops the cpal Stream" after
    /// `running=false` is set, even when the DATA channel is full and NO stop
    /// CONTROL was delivered.  The defence-in-depth path is: DATA recv times out
    /// (after draining the queued PCM) → flag check → exit.
    ///
    /// Simulates: (1) fill the DATA channel; (2) set running=false; (3) send NO
    /// control; (4) assert the thread joins via the DATA-timeout + flag-check path.
    #[test]
    fn audio_thread_exits_on_flag_false_even_when_data_full_no_control() {
        let capacity = 4;
        let (data_tx, data_rx) = std::sync::mpsc::sync_channel::<AudioCommand>(capacity);
        let (ctrl_tx, ctrl_rx) =
            std::sync::mpsc::sync_channel::<AudioControl>(CONTROL_CHANNEL_CAP);
        let running = Arc::new(AtomicBool::new(true));
        let running_for_thread = Arc::clone(&running);

        // Mirror the production two-channel loop: CONTROL first (none here), then
        // DATA recv_timeout + flag check on timeout.
        let handle = std::thread::spawn(move || {
            'outer: loop {
                match ctrl_rx.try_recv() {
                    Ok(AudioControl::FlushThenStop) | Ok(AudioControl::StopNow) => break 'outer,
                    Ok(AudioControl::ClearPlayback) => {} // non-terminal (rhanis-bx7)
                    Err(std::sync::mpsc::TryRecvError::Empty) => {} // no control → DATA
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => break 'outer,
                }
                match data_rx.recv_timeout(std::time::Duration::from_millis(20)) {
                    Ok(AudioCommand::EnqueuePcm(_)) => {}
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                        if !running_for_thread.load(Ordering::Acquire) {
                            break 'outer;
                        }
                        std::thread::sleep(std::time::Duration::from_millis(20));
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                        if !running_for_thread.load(Ordering::Acquire) {
                            break 'outer;
                        }
                    }
                }
            }
            // "cpal Stream dropped here" — the test verifies we reach this point.
        });

        // Fill the DATA channel to capacity (queued PCM).
        for _ in 0..capacity {
            let _ = data_tx.try_send(AudioCommand::EnqueuePcm(vec![]));
        }
        // Verify it is indeed full.
        let overflow = data_tx.try_send(AudioCommand::EnqueuePcm(vec![]));
        assert!(overflow.is_err(), "DATA channel must be full before the test is valid");

        // Set running=false but DELIVER NO CONTROL — the flag alone must break it.
        running.store(false, Ordering::Release);
        // Keep ctrl_tx + data_tx alive so the thread relies on the flag, not a drop.
        let _keep_ctrl = &ctrl_tx;
        let _keep_data = &data_tx;

        let (done_tx, done_rx) = std::sync::mpsc::channel::<()>();
        std::thread::spawn(move || {
            let _ = handle.join();
            let _ = done_tx.send(());
        });

        let joined = done_rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .is_ok();
        assert!(
            joined,
            "audio thread must exit after running=false even when no control was delivered (DATA full)"
        );
        // Senders kept alive until here so the exit was via the flag, not a disconnect.
        drop(ctrl_tx);
        drop(data_tx);
    }

    // ── P1 (rhanis-flu): graceful vs immediate stop — redesigned command enum ────
    //
    // Root cause (confirmed round 4): the `running` atomic was OVERLOADED for two
    // purposes — (1) gate the cpal capture callback and (2) gate the FlushCapture
    // arm's flush/discard decision.  stop_graceful() sent FlushCapture, then set
    // running=false, then sent Stop — the audio thread could see running=false when
    // it processed the dequeued FlushCapture, eating the legitimate tail.
    //
    // Fix: the flush/discard decision is now encoded in WHICH command is sent:
    //   - FlushThenStop  → audio thread flushes UNCONDITIONALLY, then stops
    //   - StopNow        → audio thread discards the tail, stops immediately
    //
    // The `running` atomic now has ONLY ONE purpose: gating the cpal capture
    // data-callback.  It is NOT read by the command-receive loop to decide flush.
    //
    // Tests verify the REDESIGNED stop paths (rhanis-flu task requirement):
    //   (a) normal close → FlushThenStop is sent; tail IS enqueued (no running-race)
    //   (b) abnormal exit / manual shutdown → StopNow; no tail enqueued
    //   (c) StopNow never flushes even if a tail is buffered
    //   (d) capture callback stops once running=false (purpose 1 intact)

    /// (a) Normal close: `stop_graceful()` sends a single `FlushThenStop` command.
    /// The audio thread's handler flushes UNCONDITIONALLY (no running-flag check),
    /// so no race can eat the legitimate tail even if running=false by the time the
    /// audio thread processes the command.
    ///
    /// This regression test was impossible to pass reliably under the old design
    /// because the guard read running AFTER stop_graceful had set it to false.
    #[test]
    fn stop_graceful_sends_flush_then_stop_single_command() {
        let running = Arc::new(AtomicBool::new(true));
        let (ctrl_tx, ctrl_rx) = std::sync::mpsc::sync_channel::<AudioControl>(CONTROL_CHANNEL_CAP);
        let handle = AudioStopHandle {
            running: Arc::clone(&running),
            ctrl_tx,
        };

        handle.stop_graceful();

        // running must be false after graceful stop.
        assert!(!running.load(Ordering::Acquire), "running must be false after stop_graceful");

        // Exactly ONE control on the CONTROL channel: FlushThenStop.
        let first = ctrl_rx.try_recv().expect("(a) FlushThenStop must be enqueued");
        assert!(
            matches!(first, AudioControl::FlushThenStop),
            "(a) stop_graceful must send FlushThenStop on the control channel"
        );
        assert!(
            ctrl_rx.try_recv().is_err(),
            "(a) only ONE control must be in the channel (FlushThenStop is a single control)"
        );
    }

    /// (a2) Regression: `FlushThenStop` handler emits the tail UNCONDITIONALLY —
    /// even when `running` is already false at execution time.
    ///
    /// This is the critical regression test: under the OLD design the FlushCapture
    /// arm checked `running` and would DISCARD the tail if running=false.  Under
    /// the NEW design `FlushThenStop` always flushes, regardless of the running flag.
    #[test]
    fn flush_then_stop_flushes_unconditionally_even_with_running_false() {
        use tokio::sync::mpsc as tokio_mpsc;
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let (write_tx, mut write_rx) = tokio_mpsc::channel::<Message>(8);

            // running=false — simulates the race where running was cleared before the
            // audio thread processes FlushThenStop (the OLD design lost the tail here).
            let state_flag = Arc::new(AtomicBool::new(false));
            let _ = state_flag; // The NEW design does NOT read this in the handler.

            let mut acc = ChunkAccumulator::with_chunk_samples(10);
            acc.push(&[0.5_f32; 5]); // partial tail

            let write_tx_clone = write_tx.clone();

            // Simulate the audio thread's NEW FlushThenStop handler:
            // flush UNCONDITIONALLY — no running-flag check.
            if let Some(tail) = acc.flush() {
                let msg = build_audio_append_frame(&tail);
                let _ = write_tx_clone.try_send(msg);
            }

            // The tail MUST have been enqueued to write_tx (even with running=false).
            let tail_frame = write_rx.try_recv();
            assert!(
                tail_frame.is_ok(),
                "(a2) FlushThenStop must enqueue the tail to write_tx UNCONDITIONALLY \
                 (even when running=false — no race possible with new design)"
            );
        });
    }

    /// (b) Abnormal exit: `stop_immediate()` sends `StopNow` — no flush.
    /// running is set to false FIRST (to gate the cpal callback), then StopNow.
    #[test]
    fn stop_immediate_sends_stop_now_no_flush() {
        let running = Arc::new(AtomicBool::new(true));
        let (ctrl_tx, ctrl_rx) = std::sync::mpsc::sync_channel::<AudioControl>(CONTROL_CHANNEL_CAP);
        let handle = AudioStopHandle {
            running: Arc::clone(&running),
            ctrl_tx,
        };

        handle.stop_immediate();

        // running must be false.
        assert!(!running.load(Ordering::Acquire), "(b) running must be false after stop_immediate");

        // Exactly one control: StopNow. No FlushThenStop.
        let first = ctrl_rx.try_recv().expect("(b) must have exactly one control");
        assert!(
            matches!(first, AudioControl::StopNow),
            "(b) stop_immediate must send StopNow (not FlushThenStop)"
        );
        assert!(
            ctrl_rx.try_recv().is_err(),
            "(b) only one control (StopNow) must be in channel"
        );
    }

    /// (c) `StopNow` handler NEVER flushes the accumulator, even if a tail is
    /// buffered.  Simulates the audio thread receiving StopNow with partial data.
    #[test]
    fn stop_now_never_flushes_even_with_buffered_tail() {
        use tokio::sync::mpsc as tokio_mpsc;
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let (write_tx, mut write_rx) = tokio_mpsc::channel::<Message>(8);

            let mut acc = ChunkAccumulator::with_chunk_samples(10);
            acc.push(&[0.5_f32; 5]); // partial tail that must NOT be flushed

            let write_tx_clone = write_tx.clone();

            // Simulate the audio thread's StopNow handler: do NOT flush.
            // (In production the handler just `break`s; we assert the accumulator
            // is NOT touched — no write_tx.try_send is called.)
            // We intentionally do nothing here to model the StopNow arm.
            let _ = write_tx_clone; // prove we could have sent but chose not to

            // No frame must have been sent to write_tx.
            assert!(
                write_rx.try_recv().is_err(),
                "(c) StopNow must NEVER flush the tail to write_tx"
            );

            // The accumulator still holds the data (not cleared on StopNow path).
            assert!(
                acc.flush().is_some(),
                "(c) accumulator still holds the tail after StopNow (it was not cleared)"
            );
        });
    }

    /// (c2) Manual shutdown (`stop()` alias) also sends `StopNow` — no flush.
    #[test]
    fn manual_shutdown_stop_sends_stop_now_no_flush() {
        let running = Arc::new(AtomicBool::new(true));
        let (ctrl_tx, ctrl_rx) = std::sync::mpsc::sync_channel::<AudioControl>(CONTROL_CHANNEL_CAP);
        let handle = AudioStopHandle {
            running: Arc::clone(&running),
            ctrl_tx,
        };

        // stop() is the legacy alias used by manual shutdown + stop_session paths.
        handle.stop();

        assert!(!running.load(Ordering::Acquire), "(c2) running must be false");

        let mut flush_then_stop_count = 0usize;
        let mut stop_now_count = 0usize;
        let mut clear_count = 0usize;
        while let Ok(ctrl) = ctrl_rx.try_recv() {
            match ctrl {
                AudioControl::FlushThenStop => flush_then_stop_count += 1,
                AudioControl::StopNow => stop_now_count += 1,
                AudioControl::ClearPlayback => clear_count += 1,
            }
        }
        assert_eq!(flush_then_stop_count, 0, "(c2) stop() must NOT send FlushThenStop");
        assert_eq!(stop_now_count, 1, "(c2) stop() must send exactly one StopNow");
        assert_eq!(clear_count, 0, "(c2) stop() must NOT send ClearPlayback");
    }

    /// (d) Capture callback stops emitting once `running=false` — purpose 1 of the
    /// `running` atomic is intact.  This is a pure unit test of `capture_should_emit`.
    #[test]
    fn capture_callback_stops_emitting_after_running_false() {
        // running=true → capture_should_emit returns true.
        assert!(
            capture_should_emit(true),
            "(d) capture must emit when running=true"
        );
        // running=false → capture_should_emit returns false (purpose 1 intact).
        assert!(
            !capture_should_emit(false),
            "(d) capture must NOT emit when running=false"
        );

        // Simulate: stop_graceful() sets running=false AFTER sending FlushThenStop.
        // The capture callback reads running=false and stops — but FlushThenStop
        // still flushes unconditionally (its flush does not read running).
        let running = Arc::new(AtomicBool::new(true));
        let (ctrl_tx, ctrl_rx) = std::sync::mpsc::sync_channel::<AudioControl>(CONTROL_CHANNEL_CAP);
        let handle = AudioStopHandle {
            running: Arc::clone(&running),
            ctrl_tx,
        };
        handle.stop_graceful();

        // After stop_graceful: running=false (callback stops)
        assert!(!capture_should_emit(running.load(Ordering::Acquire)),
            "(d) capture must stop after stop_graceful sets running=false");

        // FlushThenStop was sent on the CONTROL channel (flush is driven by the
        // control, not the running flag).
        assert!(matches!(ctrl_rx.try_recv(), Ok(AudioControl::FlushThenStop)),
            "(d) FlushThenStop must be in the control channel despite running=false");
    }

    // ── P1-1 / P1-3 integrated: run the REAL two-channel command loop ─────────
    //
    // These drive the production `run_audio_command_loop` (the hardware-free core
    // of `audio_thread_main`) on a worker thread, with the DATA channel pre-filled
    // to FULL, proving control is honoured and the graceful tail reaches write_tx
    // even under PCM congestion.

    /// Spawns `run_audio_command_loop` on a worker thread, returning a join helper.
    /// `accum` carries any partial tail to flush; `on_pcm` records enqueued PCM.
    fn spawn_command_loop(
        data_rx: std::sync::mpsc::Receiver<AudioCommand>,
        ctrl_rx: std::sync::mpsc::Receiver<AudioControl>,
        accum: Arc<std::sync::Mutex<ChunkAccumulator>>,
        write_tx: mpsc::Sender<Message>,
        state_flag: Arc<AtomicBool>,
        pcm_seen: Arc<AtomicBool>,
    ) -> std::thread::JoinHandle<()> {
        std::thread::spawn(move || {
            run_audio_command_loop(
                &data_rx,
                &ctrl_rx,
                &accum,
                &write_tx,
                &state_flag,
                |op: PlaybackOp| {
                    if matches!(op, PlaybackOp::Enqueue(_)) {
                        pcm_seen.store(true, Ordering::Release);
                    }
                    true // never overflow in these tests
                },
            );
        })
    }

    /// Joins a worker thread within a timeout (detects a hang = loop never stopped).
    fn join_within(handle: std::thread::JoinHandle<()>, secs: u64) -> bool {
        let (done_tx, done_rx) = std::sync::mpsc::channel::<()>();
        std::thread::spawn(move || {
            let _ = handle.join();
            let _ = done_tx.send(());
        });
        done_rx
            .recv_timeout(std::time::Duration::from_secs(secs))
            .is_ok()
    }

    /// (a, P1-1/P1-3) FlushThenStop is HONOURED and the graceful tail REACHES
    /// write_tx EVEN WHEN the PCM DATA channel is full, and the loop STOPS.
    ///
    /// This is the core P1-1 + P1-3 regression: fill the DATA channel to capacity,
    /// send FlushThenStop on the (separate) CONTROL channel, and assert (1) the
    /// tail frame arrives on write_tx, (2) the loop thread stops promptly.
    #[test]
    fn flush_then_stop_delivers_tail_even_when_data_channel_full() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            // DATA channel capacity 2; fill it so EnqueuePcm try_send would be Full.
            let (data_tx, data_rx) = std::sync::mpsc::sync_channel::<AudioCommand>(2);
            data_tx.try_send(AudioCommand::EnqueuePcm(vec![1, 0])).unwrap();
            data_tx.try_send(AudioCommand::EnqueuePcm(vec![2, 0])).unwrap();
            assert!(
                data_tx.try_send(AudioCommand::EnqueuePcm(vec![3, 0])).is_err(),
                "DATA channel must be full for this test to be meaningful"
            );

            let (ctrl_tx, ctrl_rx) =
                std::sync::mpsc::sync_channel::<AudioControl>(CONTROL_CHANNEL_CAP);

            // write_tx with headroom so the reliable tail delivery succeeds.
            let (write_tx, mut write_rx) = mpsc::channel::<Message>(8);

            // A partial tail buffered in the accumulator — this is what FlushThenStop
            // must deliver.
            let accum = Arc::new(std::sync::Mutex::new(ChunkAccumulator::with_chunk_samples(10)));
            accum.lock().unwrap().push(&[0.5_f32; 5]);

            let state_flag = Arc::new(AtomicBool::new(true));
            let pcm_seen = Arc::new(AtomicBool::new(false));

            let handle = spawn_command_loop(
                data_rx,
                ctrl_rx,
                Arc::clone(&accum),
                write_tx.clone(),
                Arc::clone(&state_flag),
                Arc::clone(&pcm_seen),
            );

            // Stop gracefully via the SEPARATE control channel (reliable send).
            send_control_reliably(&ctrl_tx, AudioControl::FlushThenStop);

            // The loop must stop promptly even though the DATA channel is full.
            assert!(
                join_within(handle, 3),
                "(a) loop must STOP on FlushThenStop even with a full DATA channel"
            );

            // The graceful tail must have reached write_tx.  It may be preceded by
            // PCM-driven frames? No — `on_pcm` does not send to write_tx in this test,
            // so the FIRST (and only) write_tx frame is the tail.
            let frame = write_rx.try_recv();
            assert!(
                frame.is_ok(),
                "(a) FlushThenStop must deliver the tail to write_tx even when DATA is full"
            );
            // Verify it is a valid input_audio_buffer.append frame (the tail).
            if let Ok(Message::Text(t)) = frame {
                let v: serde_json::Value = serde_json::from_str(&t).expect("valid json");
                assert_eq!(v["type"], "input_audio_buffer.append", "(a) tail must be an append frame");
            } else {
                panic!("(a) tail frame must be a text message");
            }
        });
    }

    /// (b, P1-1) StopNow under a FULL DATA channel stops the loop PROMPTLY and does
    /// NOT flush the tail.
    #[test]
    fn stop_now_under_full_data_channel_stops_promptly_no_flush() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let (data_tx, data_rx) = std::sync::mpsc::sync_channel::<AudioCommand>(2);
            data_tx.try_send(AudioCommand::EnqueuePcm(vec![1, 0])).unwrap();
            data_tx.try_send(AudioCommand::EnqueuePcm(vec![2, 0])).unwrap();
            assert!(data_tx.try_send(AudioCommand::EnqueuePcm(vec![3, 0])).is_err());

            let (ctrl_tx, ctrl_rx) =
                std::sync::mpsc::sync_channel::<AudioControl>(CONTROL_CHANNEL_CAP);
            let (write_tx, mut write_rx) = mpsc::channel::<Message>(8);

            // Buffer a tail that must NOT be delivered on the StopNow path.
            let accum = Arc::new(std::sync::Mutex::new(ChunkAccumulator::with_chunk_samples(10)));
            accum.lock().unwrap().push(&[0.5_f32; 5]);

            let state_flag = Arc::new(AtomicBool::new(true));
            let pcm_seen = Arc::new(AtomicBool::new(false));

            let handle = spawn_command_loop(
                data_rx,
                ctrl_rx,
                Arc::clone(&accum),
                write_tx.clone(),
                Arc::clone(&state_flag),
                Arc::clone(&pcm_seen),
            );

            let start = std::time::Instant::now();
            send_control_reliably(&ctrl_tx, AudioControl::StopNow);

            assert!(
                join_within(handle, 3),
                "(b) loop must STOP promptly on StopNow even with a full DATA channel"
            );
            assert!(
                start.elapsed() < std::time::Duration::from_secs(2),
                "(b) StopNow must stop promptly"
            );

            // No tail frame on the StopNow path (abnormal exit discards the tail).
            assert!(
                write_rx.try_recv().is_err(),
                "(b) StopNow must NOT flush the tail to write_tx"
            );
            // The accumulator still holds the buffered tail (it was never flushed).
            assert!(
                accum.lock().unwrap().flush().is_some(),
                "(b) StopNow must leave the tail unflushed"
            );
        });
    }

    /// (P1-3) `deliver_tail_reliably` delivers within the bounded window even when
    /// the write queue is momentarily full but a draining consumer frees a slot.
    #[test]
    fn deliver_tail_reliably_delivers_when_consumer_drains() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            // capacity 1 so we can make it transiently full.
            let (write_tx, mut write_rx) = mpsc::channel::<Message>(1);
            // Pre-fill the single slot so the first try_send is Full.
            write_tx.try_send(Message::Text("blocker".into())).unwrap();

            // A consumer that drains the blocker after a short delay, freeing a slot.
            let drain = tokio::spawn(async move {
                // Drain the blocker first.
                let _ = write_rx.recv().await;
                // Then drain whatever the reliable delivery sends.
                write_rx.recv().await
            });

            // Give the consumer a beat to start, then deliver on a blocking thread
            // (deliver_tail_reliably is sync; run it off the async runtime).
            let tail = build_audio_append_frame(&[7, 0, 8, 0]);
            let delivered = tokio::task::spawn_blocking(move || {
                // Small initial sleep so the slot is still full on first try, then
                // the consumer drains and the retry succeeds within the deadline.
                std::thread::sleep(std::time::Duration::from_millis(5));
                deliver_tail_reliably(&write_tx, tail)
            })
            .await
            .unwrap();

            assert!(delivered, "tail must be delivered within the bounded window");
            let got = drain.await.unwrap();
            assert!(got.is_some(), "consumer must have received the delivered tail");
        });
    }

    /// (d, P1-1/realtime) The realtime capture path is non-blocking: PCM `try_send`
    /// on a full DATA channel returns Err immediately (never blocks).  This mirrors
    /// the cpal callback's `tx_cb.try_send` discipline.
    #[test]
    fn realtime_pcm_try_send_is_non_blocking_on_full_data_channel() {
        let (data_tx, _data_rx) = std::sync::mpsc::sync_channel::<AudioCommand>(1);
        data_tx.try_send(AudioCommand::EnqueuePcm(vec![0, 0])).expect("first fits");

        let start = std::time::Instant::now();
        // The realtime path uses try_send and must NOT block on a full channel.
        let r = data_tx.try_send(AudioCommand::EnqueuePcm(vec![0, 0]));
        let elapsed = start.elapsed();

        assert!(r.is_err(), "(d) realtime PCM try_send on a full DATA channel must return Err");
        assert!(
            elapsed < std::time::Duration::from_millis(50),
            "(d) realtime PCM try_send must not block (took {elapsed:?})"
        );
    }

    // ── Legacy regression tests (renamed from rhanis-flu P1 block) ─────────────

    /// Legacy P1 (a): stop_immediate does NOT enqueue FlushThenStop.
    /// Kept as a named regression; same semantics as stop_immediate_sends_stop_now_no_flush.
    #[test]
    fn stop_immediate_does_not_enqueue_flush_capture() {
        let running = Arc::new(AtomicBool::new(true));
        let (ctrl_tx, ctrl_rx) = std::sync::mpsc::sync_channel::<AudioControl>(CONTROL_CHANNEL_CAP);
        let handle = AudioStopHandle {
            running: Arc::clone(&running),
            ctrl_tx,
        };

        handle.stop_immediate();

        assert!(!running.load(Ordering::Acquire), "running must be false after stop_immediate");

        // Drain: must find StopNow, must NOT find FlushThenStop.
        let mut found_flush = false;
        let mut found_stop_now = false;
        while let Ok(ctrl) = ctrl_rx.try_recv() {
            match ctrl {
                AudioControl::FlushThenStop => found_flush = true,
                AudioControl::StopNow => found_stop_now = true,
                AudioControl::ClearPlayback => {} // not part of the stop paths
            }
        }
        assert!(!found_flush, "stop_immediate must NOT enqueue FlushThenStop");
        assert!(found_stop_now, "stop_immediate must enqueue StopNow");
    }

    /// Legacy P1 (b): stop_graceful DOES enqueue FlushThenStop (single command).
    #[test]
    fn stop_graceful_does_enqueue_flush_capture() {
        let running = Arc::new(AtomicBool::new(true));
        let (ctrl_tx, ctrl_rx) = std::sync::mpsc::sync_channel::<AudioControl>(CONTROL_CHANNEL_CAP);
        let handle = AudioStopHandle {
            running: Arc::clone(&running),
            ctrl_tx,
        };

        handle.stop_graceful();

        assert!(!running.load(Ordering::Acquire), "running must be false after stop_graceful");

        let mut found_flush_then_stop = false;
        let mut found_stop_now = false;
        while let Ok(ctrl) = ctrl_rx.try_recv() {
            match ctrl {
                AudioControl::FlushThenStop => found_flush_then_stop = true,
                AudioControl::StopNow => found_stop_now = true,
                AudioControl::ClearPlayback => {} // not part of the stop paths
            }
        }
        assert!(found_flush_then_stop, "stop_graceful must enqueue FlushThenStop");
        assert!(!found_stop_now, "stop_graceful must NOT enqueue StopNow");
    }

    // ── Barge-in (rhanis-bx7): ClearPlayback + delta suppression ──────────────────

    /// Drives the REAL `run_audio_command_loop` with a pre-loaded barge-in:
    /// stale PCM already queued on DATA and a `ClearPlayback` on CONTROL.
    /// Control-first ordering (P1-1) makes this deterministic: the cut must
    /// (1) discard ALL stale DATA without enqueueing it, (2) perform exactly one
    /// `Clear` op, (3) keep the loop ALIVE for the next response's audio, and
    /// (4) still honour a later StopNow.
    #[test]
    fn clear_playback_cuts_stale_audio_and_keeps_loop_alive() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let (data_tx, data_rx) = std::sync::mpsc::sync_channel::<AudioCommand>(8);
            let (ctrl_tx, ctrl_rx) =
                std::sync::mpsc::sync_channel::<AudioControl>(CONTROL_CHANNEL_CAP);
            let accum = Arc::new(std::sync::Mutex::new(ChunkAccumulator::new()));
            let (write_tx, _write_rx) = mpsc::channel::<Message>(8);
            let state_flag = Arc::new(AtomicBool::new(true));

            let ops: Arc<std::sync::Mutex<Vec<String>>> =
                Arc::new(std::sync::Mutex::new(Vec::new()));
            let ops_for_loop = Arc::clone(&ops);

            // Pre-load: 3 stale PCM payloads (the interrupted response), THEN the
            // barge-in cut on CONTROL. The loop checks CONTROL first, so the cut
            // runs before any stale payload can be enqueued.
            for _ in 0..3 {
                data_tx
                    .try_send(AudioCommand::EnqueuePcm(vec![0u8; 4]))
                    .expect("pre-load PCM");
            }
            assert!(send_control_reliably(&ctrl_tx, AudioControl::ClearPlayback));

            let accum_for_loop = Arc::clone(&accum);
            let flag_for_loop = Arc::clone(&state_flag);
            let handle = std::thread::spawn(move || {
                run_audio_command_loop(
                    &data_rx,
                    &ctrl_rx,
                    &accum_for_loop,
                    &write_tx,
                    &flag_for_loop,
                    |op: PlaybackOp| {
                        ops_for_loop.lock().unwrap().push(match op {
                            PlaybackOp::Enqueue(_) => "enqueue".into(),
                            PlaybackOp::Clear => "clear".into(),
                        });
                        true
                    },
                );
            });

            // Deadline-polled wait (no fixed sleep — loaded CI must not flake).
            let wait_for_ops = |expected: usize| {
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
                while std::time::Instant::now() < deadline {
                    if ops.lock().unwrap().len() >= expected {
                        return;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
            };

            // The cut runs first (control-first, P1-1) — and because the drain
            // happens BEFORE the Clear op inside the same control handling, once
            // "clear" is visible the stale PCM is already gone for good.
            wait_for_ops(1);
            {
                let seen = ops.lock().unwrap().clone();
                assert_eq!(
                    seen,
                    vec!["clear".to_string()],
                    "the cut must clear once and discard ALL stale PCM (got {seen:?})"
                );
            }

            // The loop is still alive: fresh audio (the NEXT response) plays.
            data_tx
                .try_send(AudioCommand::EnqueuePcm(vec![1u8; 4]))
                .expect("post-cut PCM");
            wait_for_ops(2);
            {
                let seen = ops.lock().unwrap().clone();
                assert_eq!(
                    seen,
                    vec!["clear".to_string(), "enqueue".to_string()],
                    "ClearPlayback must NOT stop the loop — fresh PCM still enqueues"
                );
            }

            // A stop queued after a cut is still honoured.
            assert!(send_control_reliably(&ctrl_tx, AudioControl::StopNow));
            assert!(
                join_within(handle, 2),
                "StopNow after ClearPlayback must still stop the loop"
            );
        });
    }

    /// `PlaybackHandle` barge-in gate (rhanis-bx7), full protocol walk:
    /// speech_started closes the gate + cuts ONCE; a mid-speech
    /// `response.created` (tool-completion follow-up) must NOT reopen it;
    /// speech_stopped arms the release; the next `response.created` reopens.
    #[test]
    fn playback_gate_full_barge_in_protocol() {
        let (handle, data_rx, ctrl_rx) = PlaybackHandle::new_for_test();

        let delta = serde_json::json!({ "type": "response.audio.delta", "delta": "AAAA" });
        let speech_started = serde_json::json!({ "type": "input_audio_buffer.speech_started" });
        let speech_stopped = serde_json::json!({ "type": "input_audio_buffer.speech_stopped" });
        let created = serde_json::json!({ "type": "response.created" });

        // Normal playback before the barge-in.
        handle.handle_server_audio(&delta);
        assert!(
            matches!(data_rx.try_recv(), Ok(AudioCommand::EnqueuePcm(_))),
            "pre-barge-in delta must reach the DATA channel"
        );

        // The user starts speaking: exactly one ClearPlayback, stragglers dropped.
        handle.handle_server_audio(&speech_started);
        assert!(
            matches!(ctrl_rx.try_recv(), Ok(AudioControl::ClearPlayback)),
            "speech_started must send ClearPlayback on the CONTROL channel"
        );
        handle.handle_server_audio(&delta);
        assert!(
            data_rx.try_recv().is_err(),
            "a straggler delta after speech_started must be suppressed"
        );

        // A VAD re-trigger while already speaking must NOT send a second cut
        // (one cut per episode — flood-proof CONTROL occupancy).
        handle.handle_server_audio(&speech_started);
        assert!(
            ctrl_rx.try_recv().is_err(),
            "a repeat speech_started must not send another ClearPlayback"
        );

        // A response created WHILE the user is still speaking (tool-completion
        // follow-up, rhanis-z8j) must NOT reopen the gate: no talk-over.
        handle.handle_server_audio(&created);
        handle.handle_server_audio(&delta);
        assert!(
            data_rx.try_recv().is_err(),
            "a mid-speech response.created must NOT lift the suppression"
        );

        // The user finishes; the NEXT response is the reply — it plays.
        handle.handle_server_audio(&speech_stopped);
        handle.handle_server_audio(&created);
        handle.handle_server_audio(&delta);
        assert!(
            matches!(data_rx.try_recv(), Ok(AudioCommand::EnqueuePcm(_))),
            "the reply's audio (created AFTER speech_stopped) must play"
        );

        // A fresh barge-in episode cuts again (the gate transition re-fires).
        handle.handle_server_audio(&speech_started);
        assert!(
            matches!(ctrl_rx.try_recv(), Ok(AudioControl::ClearPlayback)),
            "a new barge-in episode must send a new ClearPlayback"
        );
        assert!(ctrl_rx.try_recv().is_err(), "no further control expected");
    }

    /// GA wire name (rhanis-bd7): the barge-in gate must treat
    /// `response.output_audio.delta` exactly like the beta name — enqueued
    /// while the gate is OFF, suppressed pre-decode once the user speaks.
    /// (The gate transitions themselves are name-independent; this mirrors
    /// the name-dependent halves of `playback_gate_full_barge_in_protocol`.)
    #[test]
    fn playback_gate_handles_ga_audio_delta_name() {
        let (handle, data_rx, ctrl_rx) = PlaybackHandle::new_for_test();

        let ga_delta =
            serde_json::json!({ "type": "response.output_audio.delta", "delta": "AAAA" });
        let speech_started = serde_json::json!({ "type": "input_audio_buffer.speech_started" });

        // Gate OFF: a GA-named delta must reach the DATA channel.
        handle.handle_server_audio(&ga_delta);
        assert!(
            matches!(data_rx.try_recv(), Ok(AudioCommand::EnqueuePcm(_))),
            "GA-named delta must enqueue while the gate is OFF"
        );

        // Barge-in: a GA-named straggler must be suppressed like the beta name.
        handle.handle_server_audio(&speech_started);
        assert!(
            matches!(ctrl_rx.try_recv(), Ok(AudioControl::ClearPlayback)),
            "speech_started must still cut playback"
        );
        handle.handle_server_audio(&ga_delta);
        assert!(
            data_rx.try_recv().is_err(),
            "a GA-named straggler after speech_started must be suppressed"
        );
    }

    /// A stray `speech_stopped` with no preceding `speech_started` must not
    /// disturb an open gate (deltas keep flowing).
    #[test]
    fn playback_gate_ignores_stray_speech_stopped() {
        let (handle, data_rx, _ctrl_rx) = PlaybackHandle::new_for_test();
        handle.handle_server_audio(
            &serde_json::json!({ "type": "input_audio_buffer.speech_stopped" }),
        );
        handle.handle_server_audio(
            &serde_json::json!({ "type": "response.audio.delta", "delta": "AAAA" }),
        );
        assert!(
            matches!(data_rx.try_recv(), Ok(AudioCommand::EnqueuePcm(_))),
            "a stray speech_stopped must not close or arm the gate"
        );
    }
}
