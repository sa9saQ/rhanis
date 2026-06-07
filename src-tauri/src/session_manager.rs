//! WebSocket session_manager (koe-e3m): drives one OpenAI Realtime session.
//!
//! Lifecycle: `start_session` connects `wss://api.openai.com/v1/realtime?model=
//! gpt-realtime-2` with a BYOK Bearer header (the key is exposed ONLY to build
//! the handshake header — never stored, logged, or emitted), sends a
//! `session.update` carrying the dispatcher's tool schemas, then spawns:
//!   - a **read loop** that routes `response.function_call_arguments.done` to the
//!     dispatcher (koe-2gy) via [`DispatcherSeam`] and, on each `response.done`
//!     usage event, adds to a [`CostTracker`] and stops fail-closed if the
//!     monthly budget is exceeded;
//!   - a single **write task** that owns the socket sink, so concurrent dispatch
//!     tasks never interleave frames on the wire.
//! `stop_session` signals shutdown, aborts both tasks (dropping the write
//! receiver ends the writer) and the in-flight dispatch `JoinSet`.
//!
//! ## Key discipline
//! The BYOK key lives in a [`RealtimeAuth`] only long enough to build the
//! Authorization header; `RealtimeAuth` is not `Serialize`/`Clone` and its
//! `Debug` is redacted. All user-facing/log strings are fixed phrases.
//!
//! ## WSL note
//! The live socket only runs on Windows (koe-ef8). Here the loop is
//! [`run_read_loop`], generic over an abstract frame `Stream` with an injected
//! `emit` closure, so it is unit-tested by feeding synthetic frames — no socket,
//! no `AppHandle`.
//!
//! transaction N/A · idempotency_key N/A (real-time session control; the budget
//! guard stops the session, it does not write a charge).

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use futures_util::{SinkExt, Stream, StreamExt};
use serde_json::Value;
use tokio::sync::{mpsc, oneshot, Mutex as TokioMutex};
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;
use tokio_tungstenite::tungstenite::{Error as WsError, Message};

use crate::audio_bridge::{ManagedAudioBridge, MAX_WS_TEXT_BYTES};
use crate::cost_tracker::{BudgetConfig, CostSnapshot, CostTracker};
use crate::events::{ManagedSequenceCounter, SequenceCounter};
use crate::realtime_provider::{select_provider, ProviderEvent, RealtimeAuth, RealtimeProvider};
use crate::realtime_types::{DispatcherSeam, FunctionCall, ManagedDispatcher};
use crate::secret_store::{ManagedSecretStore, OPENAI_KEY_NAME};
use crate::settings_store::ManagedSettings;
use crate::storage::adapter::{ManagedRecorder, RecorderAdapter};
use crate::tool_dispatcher::MAX_TOOL_NAME_LEN;

/// Hard session cap (also a coarse cost backstop). Mirrors CLAUDE.md's 30 min.
const SESSION_TIMEOUT: Duration = Duration::from_secs(30 * 60);
const WRITE_CHANNEL_CAP: usize = 32;
/// Upper bound on concurrently in-flight tool dispatches (DoS guard, koe-wj2).
///
/// A hostile / compromised model server could stream `function_call` frames for
/// the whole [`SESSION_TIMEOUT`] window. Each accepted frame spawns a task that
/// holds its captured arguments (each ≤ `MAX_ARGS_LEN`) and drives a real tool
/// side effect (file write / screenshot / shell / computer_use). The bound is on
/// those **concurrently-executing side effects and their argument buffers**, not
/// on the (tiny) `JoinSet` task handles — so do not raise it on the reasoning
/// that "handles are cheap". 64 is far above any legitimate concurrency (the
/// model issues at most a handful of parallel tool calls), so a non-hostile
/// session never reaches it.
///
/// Past the cap, new function-call frames are skipped — the session keeps
/// running (fail-soft, no crash) and resumes dispatching as earlier tasks finish
/// and are reaped. A skipped frame's `call_id` receives **no**
/// `function_call_output` (the model's pending call is intentionally left
/// unanswered); this is the deliberate fail-soft contract under attack.
///
/// Known gap (tracked: koe-rxh): a DANGER-tier dispatch parks its slot for up to
/// the 30s approval-gate timeout, so a burst of DANGER calls can hold the cap and
/// starve subsequent calls for that window. Bounding *pending approvals*
/// separately is approval_gate's concern, out of scope for this session-loop cap.
const MAX_INFLIGHT_DISPATCHES: usize = 64;
/// Consecutive cost-snapshot save failures tolerated before stopping fail-closed
/// (a persistent failure means a restart could lose the running total).
const MAX_SNAPSHOT_SAVE_FAILURES: u32 = 3;

/// Spend (nanodollars) counted into the budget gate but not yet durably written to
/// the additive ledger because an `add_month_cost` failed transiently, tagged with
/// the month it belongs to. Carried across frames so the next add retries the WHOLE
/// unpersisted amount (and the gate keeps counting it); a failed add must never
/// silently drop spend (that would undercount / fail-open). Reset to 0 once an add
/// succeeds. The `month` SCOPES the carry: if the month rolls over while spend is
/// still unpersisted, the loop fails closed rather than fold a past month's spend
/// into the new month's row (koe-ixt). (koe-ixt R-C / Codex P2)
#[derive(Default)]
struct PendingCost {
    month: u32,
    nanodollars: u64,
}

/// WebSocket frame/message size limits (DoS guard).
/// Max message: 512 KiB — comfortably above the largest legitimate Realtime
/// frame (audio deltas are ~256 KiB max; control frames are much smaller).
/// Max frame: same cap; the Realtime API does not fragment messages.
const WS_MAX_MESSAGE_SIZE: usize = 512 * 1024;
const WS_MAX_FRAME_SIZE: usize = 512 * 1024;

// ---- managed session state ---------------------------------------------------

/// In-flight session handles. `None` when idle.
pub(crate) struct ActiveSession {
    /// Monotonic generation minted by [`ManagedSession`] for this session
    /// (koe-ego). The read loop clears the slot / emits the terminal idle only
    /// while the slot still holds *this* generation, so a stop->start handover
    /// (slot taken, then a new session stored) cannot have the old loop's
    /// teardown clear the newer session's handle.
    generation: u64,
    shutdown_tx: oneshot::Sender<()>,
    write_handle: tokio::task::JoinHandle<()>,
}

/// Tauri managed state: the single optional active session plus the monotonic
/// generation source. The `tokio::Mutex` is held across the whole `start_session`
/// setup so two concurrent starts cannot both pass the `is_some()` check
/// (double-start race). Field `.0` is `pub(crate)` (not `pub`) because
/// `ActiveSession` is crate-private; field `.1` is the generation counter
/// (koe-ego) — `start_session` mints from it and the read loop reads it to ask
/// "has a newer start begun since mine?" at its terminal slot-clear. Read only
/// inside this module.
pub struct ManagedSession(
    pub(crate) Arc<TokioMutex<Option<ActiveSession>>>,
    Arc<AtomicU64>,
);

impl ManagedSession {
    pub fn new() -> Self {
        // Generations start at 1 so 0 never collides with a live session (0 reads
        // as "no session has started yet"). `Arc` so the read loop can hold the
        // counter to check whether a newer start has begun since its own.
        Self(Arc::new(TokioMutex::new(None)), Arc::new(AtomicU64::new(1)))
    }
}

// ---- helpers -----------------------------------------------------------------

/// Emits a `session-status` event (channel + shape per types.ts
/// `SessionStatusEvent`). `error` is always present in JSON (null when absent).
fn emit_session_status(
    app: &tauri::AppHandle,
    seq: &SequenceCounter,
    state: &str,
    error: Option<&str>,
) {
    use tauri::Emitter;
    let payload = serde_json::json!({
        "state": state,
        "error": error,
        "sequence": seq.next(),
    });
    let _ = app.emit("session-status", payload);
}

/// Current year-month as `YYYYMM` from the system clock, via the Howard Hinnant
/// "civil from days" algorithm — pure integer math, no `chrono`.
fn current_yyyymm() -> u32 {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let z = (secs / 86_400) as i64 + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as i64; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    let year = if m <= 2 { y + 1 } else { y };
    (year as u32) * 100 + m as u32
}

// ---- read loop (AppHandle-free; unit-tested via injected frames + emit) ------

/// What the loop should do after handling one frame.
enum LoopAction {
    Continue,
    /// Stop the loop fail-closed. The `&'static str` is the terminal error reason;
    /// it is NOT emitted here but carried out to `finalize_session_slot`, which
    /// emits it under the slot lock (generation-guarded) so a stop->start handover
    /// cannot flash a dying loop's `error` over a newer, connected session
    /// (koe-ego).
    Stop(&'static str),
}

/// Poll interval for detecting a mic device failure via `mic_running` flag.
/// 100ms is fast enough for UX feedback and cheap enough to not measurably
/// impact the audio pipeline.
const MIC_POLL_INTERVAL: Duration = Duration::from_millis(100);

/// Terminal slot handling for a read loop: clears the slot AND emits the single
/// terminal status (`error` or `idle`) atomically under the slot lock — the
/// generation guard that closes the `stop_session`→`start_session` handover race
/// (koe-ego). This is the ONLY place run_read_loop emits a terminal status, so
/// **every** terminal `error`/`idle` is generation-guarded, not just `idle`.
///
/// `terminal_error`: `Some(reason)` for an abnormal exit (budget / timeout / mic
/// lost / connection error / cost-tracking unavailable) → emit `error(reason)`;
/// `None` for a clean stop/close → emit `idle`. Three cases, decided under the
/// lock:
///
/// - slot holds **our** `generation` → take it (clear) and emit the terminal
///   status (error-or-idle);
/// - slot is **empty** → `stop_session` took our handle (or we cleared it on a
///   prior pass). We own the terminal transition only if no newer start has begun
///   since ours — checked via `latest_generation` (the counter has not advanced
///   past us). A newer start — even one that FAILED before storing an
///   `ActiveSession` — advanced the counter and owns its own transition, so we
///   stay silent (else our `idle`/`error` could land over its `connecting`/`error`,
///   e.g. wrongly clearing a failed reconnect to idle during a restart);
/// - slot holds a **different** generation → a newer session replaced us during a
///   handover; leave its handle intact and emit **nothing** (that session owns its
///   own terminal transition). Clearing it would orphan a live WS (unstoppable →
///   BYOK double-charge); emitting our status would flash the UI to `error`/`idle`
///   over a connected session (and a stale `error` is sticky + disables the stop
///   control for the live session — see sessionStore).
///
/// The clear and the emit happen under the same lock so the slot state and the
/// emitted status can never disagree. `start_session` emits `connecting` while
/// holding this same slot mutex (it is held across the whole start setup), so the
/// two are serialized: either this finalize runs first (emits our status, clears,
/// releases → a racing `connecting` follows it) or `start_session` runs first
/// (stores the newer generation → this finalize hits the different-generation arm
/// and emits nothing). Either way a stale status can never land after the newer
/// session's `connecting`. (The frontend also dedups by `sequence` as a backstop.)
/// `emit` is synchronous, so the async lock is never held across an `.await`
/// (no deadlock; minimal hold).
async fn finalize_session_slot<F>(
    session: &Arc<TokioMutex<Option<ActiveSession>>>,
    generation: u64,
    latest_generation: &AtomicU64,
    terminal_error: Option<&str>,
    emit: &F,
) where
    F: Fn(&str, Option<&str>),
{
    let mut guard = session.lock().await;
    let owns_terminal = match guard.as_ref().map(|s| s.generation) {
        Some(g) if g == generation => {
            *guard = None;
            true
        }
        // Slot empty: stop_session took our handle (or we cleared it on a prior
        // pass). We own the terminal transition only if NO newer start has begun
        // since ours. `start_session` mints our generation then leaves the counter
        // exactly one higher, so `latest == generation + 1` means we are still the
        // latest start. A newer start advances the counter the moment it passes the
        // is_some() check (the mint is before every fallible step), so we stay
        // silent for ANY newer start attempt: whether it (a) rejects at an input
        // gate (settings / onboarding / provider / budget / key) and surfaces via
        // the frontend's invoke-rejection, (b) fails connect/setup/audio and emits
        // its own backend `error`, or (c) succeeds. In every case that newer
        // attempt's outcome — not our stale `idle`/`error` — must own the UI (a
        // failed reconnect during a restart must not be cleared to idle). The
        // counter read is under this slot lock (the same mutex the mint runs under),
        // so it observes every prior mint.
        None => latest_generation.load(Ordering::Relaxed) == generation + 1,
        Some(_) => false,
    };
    if owns_terminal {
        match terminal_error {
            Some(reason) => emit("error", Some(reason)),
            None => emit("idle", None),
        }
    }
}

/// Live "what koe is about to do" disclosure emitted on the `thinking-event`
/// channel (glass-box M1, koe-sua.1). Field names are camelCased to match
/// `ThinkingEvent` in `src/features/activity/types.ts`.
///
/// Verifiable-action-first redaction: EVERY field is derived from the tool NAME
/// (a safe, bounded identifier) — never from the tool ARGUMENTS, the model's raw
/// chain-of-thought, a path, or the BYOK key. `plan` / `source` come from a fixed
/// tool→label table (the same redaction discipline as the dispatcher's
/// tool-name-derived `displaySummary`); an unknown / oversized name falls back to
/// a generic phrase and a length-bounded `tool`, so a hostile model cannot drive
/// arguments, secrets, or an oversized string into the payload. The calibrated
/// confidence label (koe-sua.2) is deliberately absent — the calibration layer
/// that would earn it does not exist yet, so M1 never fabricates one.
///
/// transaction N/A · idempotency_key N/A (display-only disclosure, not billing).
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ThinkingEvent {
    event_id: String,
    action_id: String,
    sequence: u64,
    /// M1 emits only `"deciding"` (the model chose an action and is about to act).
    phase: String,
    plan: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source: Option<String>,
    timestamp: i64,
}

impl ThinkingEvent {
    /// Builds a disclosure for an imminent tool call from its NAME only (never its
    /// arguments). `sequence` shares the global counter, minted BEFORE the dispatch
    /// is spawned, so a disclosure always sorts below the `tool-event` start it
    /// precedes (the frontend's cross-stream ordering invariant). `event_id` is
    /// derived from that globally-unique sequence — uniqueness is all the
    /// display-only dedup needs (this id is never echoed back like an approval id).
    fn for_tool(call_id: &str, tool_name: &str, sequence: u64) -> Self {
        let (plan, source) = disclosure_for_tool(tool_name);
        // Bound the displayed tool name (char-wise so UTF-8 is never split) the same
        // way the dispatcher/journal bound it, so a hostile oversized name cannot
        // bloat the payload. Empty name → no `tool` field.
        let tool = if tool_name.is_empty() {
            None
        } else {
            Some(tool_name.chars().take(MAX_TOOL_NAME_LEN).collect::<String>())
        };
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
            .unwrap_or(0);
        ThinkingEvent {
            event_id: format!("think-{sequence}"),
            action_id: call_id.to_string(),
            sequence,
            phase: "deciding".to_string(),
            plan,
            tool,
            source,
            timestamp,
        }
    }
}

/// Maps a tool NAME to a redacted, human-safe disclosure: the action phrase the
/// operator sees ("ウェブを検索しています") and the coarse source kind ("web"). This is
/// the SAME redaction discipline as the dispatcher's tool-name-derived
/// `displaySummary` — derived from the (safe, bounded) tool name, never the
/// arguments. An unknown name falls back to a generic phrase with no source, so a
/// new / hostile tool name still produces a safe, non-leaking disclosure.
fn disclosure_for_tool(tool_name: &str) -> (String, Option<String>) {
    let (plan, source): (&str, Option<&str>) = match tool_name {
        "web_search" => ("ウェブを検索しています", Some("web")),
        "read_file" => ("ファイルを読み込んでいます", Some("ファイル")),
        "take_screenshot" => ("画面を確認しています", Some("画面")),
        "write_note" => ("ノートに書き留めています", Some("ノート")),
        "write_file" => ("ファイルに書き込もうとしています", Some("ファイル")),
        "open_url" => ("リンクを開こうとしています", Some("web")),
        "open_app" => ("アプリを起動しようとしています", None),
        "run_command" => ("コマンドを実行しようとしています", None),
        "delete_file" => ("ファイルを削除しようとしています", Some("ファイル")),
        _ => ("ツールを使おうとしています", None),
    };
    (plan.to_string(), source.map(str::to_string))
}

/// The session read loop. Generic over the frame source `S` and an `emit`
/// closure `F` so it runs with no live socket and no `AppHandle` in tests.
///
/// `audio_handler`: a closure called for every server text frame so the
/// audio bridge can intercept `response.audio.delta` events.  Injected rather
/// than taking a direct `Arc<AudioBridge>` reference so the tests can provide
/// a no-op without a live audio device.
///
/// `mic_running`: a clonable `Arc<AtomicBool>` that the cpal `error_callback`
/// sets to `false` when the device is lost.  Polled every
/// [`MIC_POLL_INTERVAL`].  Pass `Arc::new(AtomicBool::new(true))` in tests
/// where no real device is present.
///
/// `stop_audio`: called on EVERY exit path (error / budget / timeout /
/// shutdown / normal close) to stop the audio bridge before the loop exits.
/// This ensures cpal mic capture and the write task are torn down fail-closed
/// even when the Tauri `stop_session` command has not been called (e.g. budget
/// trip, connection error, or timeout that originates inside the read loop).
///
/// The `bool` argument is `true` for a **graceful** stop (`FlushThenStop` — flush
/// the tail, then stop) and `false` for a **fail-closed immediate** stop (`StopNow`
/// — discard the tail, stop immediately).
/// The caller (the closure built in `start_session`) maps this to
/// `AudioStopHandle::stop_graceful()` / `stop_immediate()`.
///
/// `writer_abort`: an `Option<tokio::task::AbortHandle>` for the WS write
/// task.  On **abnormal** exits (budget trip / WS error / timeout / mic lost)
/// the writer is **aborted** so already-queued PCM is discarded immediately.
/// On a **normal** server-close exit the `Option` is `None` (or the handle is
/// not aborted) so the writer drains gracefully before being dropped.
///
/// P1 fix: previously the write task's `JoinHandle` was simply dropped on
/// abnormal exits, which does NOT cancel the task — tokio only cancels a task
/// when its `AbortHandle::abort()` is called.  This meant already-queued PCM
/// could still be flushed after an abnormal stop.
#[allow(clippy::too_many_arguments)]
async fn run_read_loop<S, F, EC, ET, A, SA>(
    mut stream: S,
    provider: Arc<dyn RealtimeProvider>,
    write_tx: mpsc::Sender<Message>,
    cost: Arc<TokioMutex<CostTracker>>,
    recorder: Arc<dyn RecorderAdapter>,
    dispatcher: Arc<dyn DispatcherSeam>,
    mut shutdown: oneshot::Receiver<()>,
    emit: F,
    session: Arc<TokioMutex<Option<ActiveSession>>>,
    generation: u64,
    latest_generation: Arc<AtomicU64>,
    audio_handler: A,
    mic_running: Arc<AtomicBool>,
    stop_audio: SA,
    writer_abort: Option<tokio::task::AbortHandle>,
    // Live cost emitter (koe-9xi): called on each usage frame with the authoritative
    // (month, cross-session total, budget) so `start_session` can push a `cost-update`
    // to the UI. Injected (not an `AppHandle`) so the loop stays unit-testable with a
    // no-op closure — the same AppHandle-free discipline as `emit` / `audio_handler`.
    emit_cost: EC,
    // Pre-tool thinking disclosure emitter (glass-box M1, koe-sua.1): called with
    // (call_id, tool_name) when a function call arrives, BEFORE the dispatch is
    // spawned, so `start_session` can push a redacted `thinking-event` to the UI.
    // Injected (not an `AppHandle`) for the same AppHandle-free unit-testability as
    // `emit` / `emit_cost`. The tool ARGUMENTS are deliberately NOT passed — the
    // closure derives a safe disclosure from the name alone (verifiable-action-first).
    emit_thinking: ET,
) where
    S: Stream<Item = Result<Message, WsError>> + Unpin,
    F: Fn(&str, Option<&str>),
    EC: Fn(u32, u64, BudgetConfig),
    ET: Fn(&str, &str),
    A: Fn(&serde_json::Value),
    SA: Fn(bool), // true = graceful (flush tail), false = immediate (discard tail)
{
    // Tracks in-flight tool dispatches so a budget trip / stop aborts them too
    // (rather than letting them complete and spend more).
    let mut dispatch_tasks = tokio::task::JoinSet::new();
    let mut save_failures: u32 = 0;
    // Unpersisted spend carried across frames after a transient ledger-add failure
    // (see [`PendingCost`]). Month-scoped so a rollover with unpersisted spend fails
    // closed instead of mis-attributing it (koe-ixt R-C).
    let mut pending = PendingCost::default();
    // Latch so the in-flight dispatch cap logs once per saturation episode, not
    // once per dropped frame — a sustained flood must not turn the fail-soft drop
    // into a stderr-backpressure DoS (koe-wj2 R-C / Codex).
    let mut cap_warned = false;
    // Same latch discipline for journal-channel drops (koe-emd / CR): surface a
    // dropped conversation record once per episode without a per-drop flood.
    let mut journal_drop_warned = false;
    let deadline = tokio::time::sleep(SESSION_TIMEOUT);
    tokio::pin!(deadline);
    // Interval-based poll for cpal device loss (error_callback sets running=false).
    let mut mic_poll = tokio::time::interval(MIC_POLL_INTERVAL);
    mic_poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    // Whether to abort in-flight tool dispatches on exit. A *deliberate* stop
    // (shutdown / budget trip / timeout / connection error) aborts them so none
    // completes and spends after the decision to stop. A *normal* server close
    // drains them instead, so their side effects (e.g. a note write) and final
    // response frames complete rather than being killed mid-flight.
    let abort_inflight: bool;
    // Terminal status carried out to `finalize_session_slot`, which emits it under
    // the slot lock (generation-guarded — koe-ego): `Some(reason)` for an abnormal
    // exit (budget / timeout / mic lost / connection error / cost-tracking
    // unavailable) → `error(reason)`; `None` for a clean stop/close → `idle`. Set
    // by the loop's error arms below. Emitting here would leak the dying loop's
    // status over a newer session during a stop->start handover.
    let mut terminal_error: Option<&'static str> = None;

    // Pre-loop check: if the mic is already not running when we enter (e.g., the
    // error_callback fired before or during start_session), fail immediately rather
    // than waiting for the first 100ms interval tick.
    if !mic_running.load(Ordering::Acquire) {
        // P1: abort the writer FIRST so no already-queued PCM is flushed, then
        // stop_immediate (StopNow — discard tail) because this is an abnormal exit.
        if let Some(h) = &writer_abort {
            h.abort();
        }
        stop_audio(false); // false = immediate (no tail flush)
        // Emit the terminal `error` under the slot lock (generation-guarded —
        // koe-ego), and only for our own slot: a stop->start handover must not
        // flash this dying loop's error over a newer, connected session.
        finalize_session_slot(&session, generation, &latest_generation, Some("mic device lost"), &emit).await;
        return;
    }

    // Conversation journal (koe-emd): records flow to a single writer task so a
    // SQLite write never blocks the read loop and the function-call hot path
    // gains no `await` (which would perturb the koe-wj2 in-flight cap). Bounded
    // so a hostile model cannot grow journal memory without bound; on overflow a
    // record is dropped (fail-soft) rather than stalling the loop.
    let (rec_tx, rec_rx) = mpsc::channel::<ConversationRecord>(CONVERSATION_LOG_CAP);
    let conversation_writer = spawn_conversation_writer(Arc::clone(&recorder), rec_rx);

    loop {
        tokio::select! {
            _ = &mut shutdown => {
                abort_inflight = true;
                break;
            }
            _ = &mut deadline => {
                terminal_error = Some("session timeout");
                abort_inflight = true;
                break;
            }
            // Poll the cpal AtomicBool; if the error_callback has fired (device
            // unplugged, driver error), stop the session fail-closed rather than
            // silently continuing as a deaf text-only session.
            _ = mic_poll.tick() => {
                if !mic_running.load(Ordering::Acquire) {
                    terminal_error = Some("mic device lost");
                    abort_inflight = true;
                    break;
                }
            }
            frame = stream.next() => {
                match frame {
                    Some(Ok(Message::Text(txt))) => {
                        // Reject oversized text frames before any allocation-heavy
                        // processing (DoS guard: a crafted frame cannot force a
                        // multi-MB serde_json parse).
                        if txt.len() > MAX_WS_TEXT_BYTES {
                            eprintln!("[session] oversized text frame ({} bytes), dropping", txt.len());
                            continue;
                        }
                        // Parse the frame ONCE: the audio bridge gets first look
                        // (so `response.audio.delta` reaches the playback queue),
                        // then the normalized dispatch path. Unparseable frames are
                        // ignored and the loop continues (no double serde parse).
                        let Ok(event) = serde_json::from_str::<serde_json::Value>(txt.as_str())
                        else {
                            continue;
                        };
                        audio_handler(&event);
                        match handle_text(
                            &event, &provider, &write_tx, &cost, &recorder, &rec_tx,
                            &dispatcher, &mut dispatch_tasks, &mut save_failures,
                            &mut pending, &mut cap_warned, &mut journal_drop_warned,
                            &emit_cost, &emit_thinking,
                        ).await {
                            LoopAction::Continue => {}
                            // Carry the terminal error reason out to finalize so it
                            // is emitted under the slot lock (generation-guarded),
                            // not here — a handover must not flash it over a newer
                            // session (koe-ego).
                            LoopAction::Stop(reason) => {
                                terminal_error = Some(reason);
                                abort_inflight = true;
                                break;
                            }
                        }
                    }
                    // Server closed, or the stream ended: normal exit — drain.
                    Some(Ok(Message::Close(_))) | None => {
                        abort_inflight = false;
                        break;
                    }
                    // Binary/ping/pong/frame — ignored; all audio arrives as text
                    // `response.audio.delta` events on the OpenAI Realtime API.
                    Some(Ok(_)) => {}
                    Some(Err(_)) => {
                        terminal_error = Some("connection error");
                        abort_inflight = true;
                        break;
                    }
                }
            }
        }
    }

    // P1 fix: on abnormal exits (budget trip / WS error / timeout / mic lost)
    // abort the WS write task FIRST (before stop_audio) so the writer cannot
    // drain any PCM from a flush. Then call stop_immediate (StopNow — no flush)
    // so the audio thread discards its tail.
    //
    // On normal server-close exits abort_inflight=false: leave the writer
    // running so it can drain gracefully, and call stop_graceful (FlushThenStop)
    // so the last speech fragment is not cut off.
    if abort_inflight {
        if let Some(h) = writer_abort {
            h.abort();
        }
        stop_audio(false); // false = immediate: StopNow (discard tail)
        dispatch_tasks.abort_all();
    } else {
        stop_audio(true); // true = graceful: flush tail
        // Drain in-flight dispatches so their side effects + final frames finish.
        while dispatch_tasks.join_next().await.is_some() {}
    }
    // Terminal slot handling (koe-ego): clear the slot and emit the single
    // terminal status (`error(reason)` for an abnormal exit, else `idle`) ONLY
    // while the slot still holds *this* session's generation (or is already empty —
    // stop_session took our handle). If a stop_session->start_session handover has
    // already replaced us with a newer generation, leave that newer handle
    // untouched and emit nothing — otherwise this exiting loop would orphan the
    // live session (its WS could no longer be stopped → BYOK double-charge) and
    // flash the UI to `error`/`idle` over a connected session. finalize is the
    // SINGLE place the read loop emits a terminal status, so every terminal
    // `error`/`idle` is generation-guarded (not just `idle`) and never doubled.
    //
    // Done under the slot lock and BEFORE the journal flush below: holding the lock
    // across the clear + emit keeps the slot state and terminal status consistent
    // (a racing start_session's `connecting` can only follow this status), and
    // clearing here (not after the conversation-writer drain) means the slot
    // handover is never delayed by journalling.
    finalize_session_slot(&session, generation, &latest_generation, terminal_error, &emit).await;
    // Close the journal channel and flush its tail before run_read_loop returns so
    // a record still in flight is persisted (the seam tests rely on this drain).
    // Unconditional: turns that already happened belong in the history even on an
    // abnormal exit. Done last (after the slot-clear) so journalling never delays
    // the session-status transition or the slot handover.
    drop(rec_tx);
    let _ = conversation_writer.await;
}

/// One conversation event queued for the journal writer (koe-emd). `role` /
/// `kind` are fixed `&'static str` labels; `summary` is owned, pre-vetted safe
/// content — a finalized transcript or a tool name, never tool arguments /
/// results, paths, or the BYOK key (the recorder stores `summary` verbatim).
struct ConversationRecord {
    role: &'static str,
    kind: &'static str,
    summary: String,
}

/// Bounded backlog for the conversation journal (koe-emd). Turns are human-paced
/// so this is never reached in normal use; it is purely a flood backstop so a
/// hostile model spamming function calls cannot grow journal memory without
/// bound. On overflow the record is dropped (best-effort journalling, fail-soft)
/// rather than blocking the read loop. See [`spawn_conversation_writer`].
const CONVERSATION_LOG_CAP: usize = 256;

/// Spawns the conversation journal writer (koe-emd): a single task that drains
/// records in send order and persists each via the recorder, so the read loop
/// never blocks on a SQLite write and never adds an `await` to the function-call
/// hot path (an `await` there would let the koe-wj2 in-flight cap reap an
/// instant dispatch and admit more than `MAX_INFLIGHT_DISPATCHES`).
///
/// Ordering: a single FIFO consumer + sequential `await` means insert order ==
/// send order == frame order, so `list_recent_events` (ordered by row id)
/// reflects the conversation timeline. Each write runs on the blocking pool
/// (mirrors `add_month_cost`) so it never blocks an async worker.
///
/// Fail-soft: a store error is logged (content-free) and skipped — a failed
/// journal write is a side effect, never a reason to stop the session (contrast
/// the cost snapshot, which is a billing safety invariant). The caller drops the
/// sender and `await`s the returned handle on loop exit so the tail is flushed.
fn spawn_conversation_writer(
    recorder: Arc<dyn RecorderAdapter>,
    mut rx: mpsc::Receiver<ConversationRecord>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        while let Some(rec) = rx.recv().await {
            let r = Arc::clone(&recorder);
            let stored = tokio::task::spawn_blocking(move || {
                r.log_conversation_event(rec.role, rec.kind, &rec.summary)
            })
            .await;
            if !matches!(stored, Ok(Ok(_))) {
                // Content-free (no role / summary / path) so a dropped write is
                // observable without leaking the turn. The writer keeps draining
                // — one bad write never abandons later records.
                eprintln!("[session] conversation event not recorded (store unavailable)");
            }
        }
    })
}

/// Enqueues a conversation record on the journal channel — non-blocking and
/// fail-soft, but NOT silent. `try_send` adds no `await` to the caller (so the
/// koe-wj2 function-call hot path is unchanged) and never blocks the read loop.
///
/// A drop is logged ONCE per episode (latched via `drop_warned`, re-armed on the
/// next successful send) so a sustained flood cannot turn the drop into a
/// stderr-backpressure DoS — while still surfacing the audit gap koe-emd exists
/// to close (a silently-dropped record is exactly the "log is empty" failure we
/// are fixing). `Closed` (writer task gone) is reported distinctly from `Full`
/// (backlog saturated under flood).
///
/// `Full` and `Closed` share one `drop_warned` latch. That is sufficient because
/// `Closed` is effectively unreachable during the read loop: the writer task
/// owns the receiver and outlives the loop (the channel only closes when the loop
/// drops `rec_tx` on exit), and the writer body cannot panic (its only fallible
/// call, the recorder write, is a `spawn_blocking` whose `JoinError`/`Err` is
/// handled, not unwrapped). So a `Full`-latched-then-`Closed` gap cannot occur in
/// practice; if the writer is ever made fallible mid-loop, split the latch (per
/// the koe-a4f follow-up).
fn enqueue_record(
    rec_tx: &mpsc::Sender<ConversationRecord>,
    record: ConversationRecord,
    drop_warned: &mut bool,
) {
    match rec_tx.try_send(record) {
        Ok(()) => *drop_warned = false,
        Err(mpsc::error::TrySendError::Full(_)) => {
            if !*drop_warned {
                eprintln!("[session] conversation journal backlog full, dropping records (writer slow)");
                *drop_warned = true;
            }
        }
        Err(mpsc::error::TrySendError::Closed(_)) => {
            if !*drop_warned {
                eprintln!("[session] conversation journal channel closed, dropping records");
                *drop_warned = true;
            }
        }
    }
}

/// Drives one decoded server frame: the provider normalizes it into zero or more
/// [`ProviderEvent`]s, each handled by [`handle_event`] in order. A single frame
/// can yield more than one event — the user `.completed` ASR frame yields a
/// `Transcript` AND a `Usage` (koe-pbe) — and the normalizer surfaces the
/// transcript FIRST, so the turn is journalled before a `Usage` budget gate could
/// stop the loop. The first `Stop` short-circuits the rest of the frame's events.
#[allow(clippy::too_many_arguments)]
async fn handle_text<EC, ET>(
    event: &Value,
    provider: &Arc<dyn RealtimeProvider>,
    write_tx: &mpsc::Sender<Message>,
    cost: &Arc<TokioMutex<CostTracker>>,
    recorder: &Arc<dyn RecorderAdapter>,
    rec_tx: &mpsc::Sender<ConversationRecord>,
    dispatcher: &Arc<dyn DispatcherSeam>,
    dispatch_tasks: &mut tokio::task::JoinSet<()>,
    save_failures: &mut u32,
    pending: &mut PendingCost,
    cap_warned: &mut bool,
    journal_drop_warned: &mut bool,
    emit_cost: &EC,
    emit_thinking: &ET,
) -> LoopAction
where
    EC: Fn(u32, u64, BudgetConfig),
    ET: Fn(&str, &str),
{
    for ev in provider.parse_frame(event) {
        if let LoopAction::Stop(reason) = handle_event(
            ev,
            write_tx,
            cost,
            recorder,
            rec_tx,
            dispatcher,
            dispatch_tasks,
            save_failures,
            pending,
            cap_warned,
            journal_drop_warned,
            emit_cost,
            emit_thinking,
        )
        .await
        {
            return LoopAction::Stop(reason);
        }
    }
    LoopAction::Continue
}

/// Handles ONE normalized [`ProviderEvent`]. Returns whether to keep looping.
/// (Extracted from `handle_text` unchanged when `parse_frame` became multi-event,
/// koe-pbe — the cost-metering `Usage` arm is byte-for-byte the koe-9xi/koe-ixt
/// logic; an ASR `Usage` flows through it identically, so there is no second cost
/// path to keep in sync.)
#[allow(clippy::too_many_arguments)]
async fn handle_event<EC, ET>(
    ev: ProviderEvent,
    write_tx: &mpsc::Sender<Message>,
    cost: &Arc<TokioMutex<CostTracker>>,
    recorder: &Arc<dyn RecorderAdapter>,
    rec_tx: &mpsc::Sender<ConversationRecord>,
    dispatcher: &Arc<dyn DispatcherSeam>,
    dispatch_tasks: &mut tokio::task::JoinSet<()>,
    save_failures: &mut u32,
    pending: &mut PendingCost,
    cap_warned: &mut bool,
    journal_drop_warned: &mut bool,
    emit_cost: &EC,
    emit_thinking: &ET,
) -> LoopAction
where
    EC: Fn(u32, u64, BudgetConfig),
    ET: Fn(&str, &str),
{
    match ev {
        ProviderEvent::FunctionCall(pending) => {
            // Reap finished dispatches so the in-flight count reflects reality,
            // then bound it (DoS guard, koe-wj2 — see MAX_INFLIGHT_DISPATCHES for
            // the threat model and the koe-rxh approval-gate caveat). The per-frame
            // argument size cap (MAX_ARGS_LEN) already ran in parse_frame (over-cap
            // frames arrive here as `Ignored`); the call's arguments are still
            // UNPARSED at this point, so a saturated burst is rejected below WITHOUT
            // paying the JSON parse — matching the pre-trait order (cap before arg
            // parse). Skipping returns LoopAction::Continue (fail-soft): the session
            // keeps running and the dropped call_id is intentionally left unanswered.
            while dispatch_tasks.try_join_next().is_some() {}
            if dispatch_tasks.len() >= MAX_INFLIGHT_DISPATCHES {
                // Log once per saturation episode (latched). A sustained flood
                // must not turn this fail-soft drop into a stderr-backpressure
                // DoS: a per-frame synchronous write could stall the read loop.
                if !*cap_warned {
                    eprintln!(
                        "[session] in-flight tool dispatch cap ({MAX_INFLIGHT_DISPATCHES}) reached, dropping calls until a slot frees"
                    );
                    *cap_warned = true;
                }
                return LoopAction::Continue;
            }
            // Back below the cap — re-arm the latch so the next saturation
            // episode logs once more.
            *cap_warned = false;

            // Journal the tool INVOCATION (koe-emd) — i.e. "the model requested
            // tool X", recorded when the function_call arrives, NOT a confirmed
            // execution outcome. A later approval-deny / policy-block / dispatch
            // error is not yet reflected here; an outcome/phase column on
            // ConversationEvent (a schema migration) is tracked as a follow-up so
            // the audit log can distinguish requested vs executed (see PR notes).
            // The tool NAME is a fixed, safe identifier — no arguments / result,
            // so no PII / secret reaches the log; the result-content path stays
            // owned by the dispatcher's own redaction. Recorded synchronously here
            // (not in the spawned dispatch task) so it keeps frame order with the
            // surrounding transcripts. The name is bounded by MAX_TOOL_NAME_LEN —
            // the same cap the dispatcher applies — so a hostile model cannot
            // drive an oversized string into a persisted row; an over-long name is
            // left unjournalled because the dispatcher will reject it ("tool name
            // too long") and it never advances the conversation.
            if pending.name.len() <= MAX_TOOL_NAME_LEN {
                enqueue_record(
                    rec_tx,
                    ConversationRecord {
                        role: "tool",
                        kind: "tool",
                        summary: pending.name.clone(),
                    },
                    journal_drop_warned,
                );
            }

            // Disclose the imminent action BEFORE the tool runs (glass-box M1,
            // koe-sua.1). Emitted synchronously here — after the in-flight cap has
            // admitted the call, but BEFORE the dispatch task is spawned — so the
            // `thinking-event` always precedes this call's `tool-event` phase=start
            // (which the dispatcher emits inside the spawned task) and lands inside
            // the 300–700ms thinking window rather than after a silent pause. Built
            // from the call_id + tool NAME only; the (still-unparsed) ARGUMENTS
            // below are never passed in, so no PII / secret can reach the disclosure.
            emit_thinking(&pending.call_id, &pending.name);

            // Only now that the cap has admitted the call do we parse the
            // (already size-capped) arguments. Default to null so a malformed blob
            // still reaches the tool, which validates its own schema.
            let args = serde_json::from_str(&pending.args_raw).unwrap_or(Value::Null);
            let call = FunctionCall {
                call_id: pending.call_id,
                name: pending.name,
                args,
            };
            let dispatcher = Arc::clone(dispatcher);
            let tx = write_tx.clone();
            dispatch_tasks.spawn(async move {
                let result = dispatcher.dispatch(call).await;
                // Bounded channel: if the writer is gone (session stopped) these
                // simply fail and the task ends.
                let _ = tx.send(Message::Text(result.conversation_item_create.to_string().into())).await;
                let _ = tx.send(Message::Text(result.response_create.to_string().into())).await;
            });
            LoopAction::Continue
        }
        ProviderEvent::Usage(usage) => {
            // This frame's incremental cost — the delta to add to the month's
            // ledger. Advance the SESSION-LOCAL tracker too (month rollover +
            // saturating add + its own view) and capture the EFFECTIVE accounting
            // month + budget config, then DROP the guard before any .await (never
            // hold the lock across an await — deadlock risk).
            //
            // We key the ledger on `c.current_month` (the month the usage was
            // actually counted into), NOT the raw observed clock month: `add_usage`
            // advances `current_month` only on a FORWARD month and otherwise keeps
            // it (a backward clock skew / NTP step does not rewind it). Keying on the
            // effective month avoids adding/gating a DIFFERENT month row and missing
            // an already-over-cap current month (fail-open) across a month boundary.
            let delta = usage.cost_nanodollars();
            let observed_month = current_yyyymm();
            let (effective_month, budget) = {
                let mut c = cost.lock().await;
                c.add_usage(&usage, observed_month);
                (c.current_month, c.config)
            };
            // The amount to add includes any earlier spend that FAILED to persist
            // (carried in `pending`), so a transient ledger failure never silently
            // drops spend: we retry the whole unpersisted amount and the gate keeps
            // counting it until an add succeeds. The carry is MONTH-SCOPED: if it
            // belongs to a PAST month (a rollover happened while spend was still
            // unpersisted), the stale carry is DROPPED rather than folded into the
            // new month's row (which would over-count the new month). Dropping it is
            // sound because the old month's cap was already enforced live, frame by
            // frame, and its persisted total is never read again once it is no longer
            // the current month (`load_cost_snapshot` is only called for the current
            // month at session start). We then proceed normally so THIS frame's spend
            // IS still recorded in — and gated against — the NEW month, instead of
            // being lost to an early stop (koe-ixt R-C / Codex P2).
            if pending.month != effective_month {
                pending.nanodollars = 0;
                pending.month = effective_month;
            }
            let to_add = pending.nanodollars.saturating_add(delta);
            // Add to the SHARED monthly ledger and read back the new authoritative
            // cross-session total (koe-ixt). Additive accounting is the single source
            // of truth: it SUMS every session's spend, so a stop->start handover
            // where an older read loop is still draining late usage cannot run a
            // newer session fail-open on a stale local baseline (mechanism 4), and two
            // overlapping sessions' spend is summed rather than one side lost to a
            // max(); it is order-independent so a late / out-of-order add never rolls
            // the total back (mechanism 5); and it saturates so it never overflows
            // (fail-closed at u64::MAX).
            let rec = Arc::clone(recorder);
            let added =
                tokio::task::spawn_blocking(move || rec.add_month_cost(effective_month, to_add)).await;
            // `durable` is true only when the add SUCCEEDED, i.e. `authoritative_total`
            // equals what a later `get_cost_snapshot` pull (reading the persisted
            // ledger) would also see. On the add-fail / readback path the total
            // includes the not-yet-persisted carried amount, so it is NOT durable.
            let (authoritative_total, durable) = match added {
                Ok(Ok(new_total)) => {
                    // The whole unpersisted amount is now durably in the ledger.
                    *save_failures = 0;
                    pending.nanodollars = 0;
                    (new_total, true)
                }
                _ => {
                    *save_failures += 1;
                    // Carry the unpersisted amount forward (tagged with its month) so
                    // the NEXT add retries it (and the gate below keeps counting it) —
                    // a failed add must never drop spend from the ledger (undercount
                    // / fail-open).
                    pending.nanodollars = to_add;
                    pending.month = effective_month;
                    if *save_failures >= MAX_SNAPSHOT_SAVE_FAILURES {
                        // Can't durably track spend → stop rather than risk a restart
                        // resetting the monthly total (fail-closed). Terminal error
                        // emitted by finalize_session_slot under the slot lock
                        // (koe-ego), not here.
                        return LoopAction::Stop("cost tracking unavailable");
                    }
                    // The ledger ADD failed but the persisted total is usually still
                    // READABLE. Gate on persisted + the (carried) unpersisted amount —
                    // a fail-closed lower bound on true spend, so a handover sibling's
                    // over-cap total still stops THIS session across a transient write
                    // failure. If the READ also fails the balance is UNKNOWN → an
                    // unknown balance must never permit continued charging (koe rule:
                    // unknown / error / timeout reject), so fail-closed stop. NOT
                    // durable: this total carries the unpersisted amount.
                    let rec_read = Arc::clone(recorder);
                    let readback = tokio::task::spawn_blocking(move || {
                        rec_read.load_cost_snapshot(effective_month)
                    })
                    .await;
                    match readback {
                        Ok(Ok(persisted)) => (persisted.unwrap_or(0).saturating_add(to_add), false),
                        _ => return LoopAction::Stop("cost tracking unavailable"),
                    }
                }
            };
            // Push the cost snapshot to the UI (koe-9xi) — but ONLY when the total is
            // DURABLE (the add succeeded), so the pushed value equals what a later
            // `get_cost_snapshot` pull (reading only the persisted ledger) would also
            // see. Emitting the NON-durable readback total (persisted + carried
            // pending) would let a subsequent pull mint a HIGHER sequence with a LOWER
            // (persisted-only) total and overwrite an over-budget display, hiding the
            // stop state after a transient SQLite-BUSY write failure (Codex Cloud P2).
            // The budget GATE below still uses the non-durable lower bound to stop
            // fail-closed; on that rare path the stop is conveyed by the session-status
            // `error`, not by repainting the cost header with a value a pull can't
            // reproduce. The two "cost tracking unavailable" hard-stops above also
            // return EARLIER without emitting (unknown balance). Emitted BEFORE the
            // gate so a durable over-budget snapshot still reaches the header. Payload
            // is numbers + a bool only (no key / path / PII).
            if durable {
                emit_cost(effective_month, authoritative_total, budget);
            }
            // Fail-closed against the AUTHORITATIVE (cross-session) ledger total, not
            // just this session's local total. Terminal error emitted by
            // finalize_session_slot under the slot lock (koe-ego), not here, so a
            // handover can't flash it over a newer session.
            if budget.is_over(authoritative_total) {
                return LoopAction::Stop("monthly budget exceeded"); // fail-closed
            }
            LoopAction::Continue
        }
        ProviderEvent::Transcript { role, text } => {
            // Journal a user / assistant speech turn (koe-emd). kind="speech"
            // matches the existing sqlite tests. Non-blocking + FIFO-ordered; a
            // full backlog drops the turn (fail-soft) so a journal write can never
            // stall or stop the conversation, but the drop is surfaced (latched)
            // rather than silent — see enqueue_record.
            enqueue_record(
                rec_tx,
                ConversationRecord {
                    role: role.as_role_str(),
                    kind: "speech",
                    summary: text,
                },
                journal_drop_warned,
            );
            LoopAction::Continue
        }
        // PR1: OpenAI's parse_frame never emits AudioDelta (the audio_handler seam
        // already consumes audio.delta); both arms simply continue. PR2 will route
        // Gemini server audio through the AudioDelta arm here.
        ProviderEvent::AudioDelta | ProviderEvent::Ignored => LoopAction::Continue,
    }
}

// ---- Tauri commands ----------------------------------------------------------

/// Starts a Realtime session. Gated on completed onboarding and a non-exceeded
/// budget; the BYOK key is fetched and used only to build the handshake header.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn start_session(
    app: tauri::AppHandle,
    session: tauri::State<'_, ManagedSession>,
    secret: tauri::State<'_, ManagedSecretStore>,
    settings: tauri::State<'_, ManagedSettings>,
    recorder: tauri::State<'_, ManagedRecorder>,
    dispatcher: tauri::State<'_, ManagedDispatcher>,
    seq: tauri::State<'_, ManagedSequenceCounter>,
    audio: tauri::State<'_, ManagedAudioBridge>,
) -> Result<(), String> {
    // Hold the lock across the whole setup so a second concurrent start cannot
    // pass the is_some() check before this one stores its session.
    let mut guard = session.0.lock().await;
    if guard.is_some() {
        return Err("session already active".to_string());
    }
    // Mint this start attempt's generation (koe-ego) BEFORE the fallible setup
    // (connect / session.update / audio), so even a start that FAILS before
    // storing an ActiveSession still advances the counter. An exiting old loop
    // reads this counter to detect that a newer start has begun and then stays
    // silent at its terminal slot-clear (the `None` arm of finalize_session_slot);
    // otherwise its `idle` could land over the newer start's `connecting`/`error`
    // — e.g. a failed reconnect during a restart would be wrongly cleared to idle.
    //
    // `Relaxed` is sufficient: the mint runs while the slot mutex `guard` is held
    // (from the is_some() check above) and the read loop reads BOTH the stored
    // generation and this counter back through the same mutex, so the mutex
    // supplies the happens-before. The atomic only hands out unique, monotonically
    // increasing ids, which a single atomic's modification order guarantees even
    // under Relaxed.
    let generation = session.1.fetch_add(1, Ordering::Relaxed);
    let latest_generation = Arc::clone(&session.1);

    let app_settings = settings.0.load().map_err(|_| "settings unavailable".to_string())?;
    if !app_settings.onboarding_completed {
        return Err("onboarding not completed".to_string());
    }

    // Resolve the voice provider from the persisted selection before touching the
    // key or the socket. `google/*` is a typed "not yet supported" error (PR2);
    // unknown values are rejected (defense-in-depth — settings already validates
    // on load). No status is emitted yet (the UI is still idle at this point).
    let provider = select_provider(&app_settings.voice_provider_model)?;

    let month = current_yyyymm();
    // Restore the running monthly total so a restart does not reset the budget.
    let rec_for_restore = Arc::clone(&recorder.0);
    let restored = tokio::task::spawn_blocking(move || rec_for_restore.load_cost_snapshot(month))
        .await
        .map_err(|_| "cost restore failed".to_string())?
        .map_err(|_| "cost restore failed".to_string())?;
    let mut tracker = CostTracker::new(app_settings.budget, month);
    if let Some(total) = restored {
        tracker.month_total_nanodollars = total;
    }
    if !tracker.can_start_session() {
        return Err("monthly budget exceeded".to_string());
    }

    // Fetch the key; expose it only to build the header, then drop `auth`.
    let key = secret
        .0
        .get_api_key(OPENAI_KEY_NAME)
        .map_err(|_| "API key not configured".to_string())?;
    let auth = RealtimeAuth::Byok(key);

    emit_session_status(&app, &seq.0, "connecting", None);

    let request = provider.build_request(&auth).map_err(|e| e.to_string())?;
    drop(auth); // the credential must not outlive header construction

    // Connect with explicit frame/message size limits so a crafted server cannot
    // cause the client to allocate more than WS_MAX_MESSAGE_SIZE bytes for a
    // single message (DoS guard).
    // Note: WebSocketConfig is `#[non_exhaustive]` so we must mutate a Default.
    let mut ws_config = WebSocketConfig::default();
    ws_config.max_message_size = Some(WS_MAX_MESSAGE_SIZE);
    ws_config.max_frame_size = Some(WS_MAX_FRAME_SIZE);
    let (ws_stream, _resp) = tokio_tungstenite::connect_async_with_config(
        request,
        Some(ws_config),
        false,
    )
    .await
    .map_err(|_| {
        emit_session_status(&app, &seq.0, "error", Some("connection failed"));
        "connection failed".to_string()
    })?;
    emit_session_status(&app, &seq.0, "connected", None);

    let (mut sink, stream) = ws_stream.split();

    // Advertise tools so the model can issue function calls (else the dispatch
    // loop is permanently idle). The provider supplies the exact setup frames
    // (OpenAI: one `session.update`); each is sent in order over the sink.
    for frame in provider.initial_frames(&dispatcher.0.tool_schemas()) {
        sink.send(frame).await.map_err(|_| {
            // Emit error status before returning so the frontend transitions out
            // of the "connected" state that was emitted at the WS-connect step.
            // Generic wording (not "session.update") since the provider may send
            // a different / multi-frame setup sequence.
            emit_session_status(&app, &seq.0, "error", Some("session setup failed"));
            "session setup failed".to_string()
        })?;
    }

    // Single writer owns the sink → concurrent dispatch tasks can't interleave.
    let (write_tx, mut write_rx) = mpsc::channel::<Message>(WRITE_CHANNEL_CAP);
    let write_handle = tokio::spawn(async move {
        while let Some(msg) = write_rx.recv().await {
            if sink.send(msg).await.is_err() {
                break;
            }
        }
    });

    // Start the audio bridge (mic capture → write_tx + server audio → rodio sink).
    // On WSL / CI this will return Err (no audio device); fail-closed rule: we
    // surface the error to the caller. On the error path we also abort write_handle
    // so the WS TCP connection is torn down promptly (avoids an OpenAI session that
    // charges against the user's quota without performing any work), and emit the
    // corrective `error` session-status event so the frontend UI transitions out of
    // the "connected" state that was emitted at the WS-connect step above.
    //
    // `start()` returns an `AudioStopHandle` — a lock-free (Arc<AtomicBool> +
    // SyncSender) pair captured at start-time. We use it in `stop_audio` so the
    // closure NEVER needs `try_lock()`. Under contention (budget-trip / mic-lost /
    // WS-error while another task holds the bridge mutex) `stop_audio()` still
    // stops the mic atomically via the atomic flag + try_send, avoiding the silent
    // skip that the old `try_lock` path could produce (P0 fix).
    let (mic_running, stop_handle) = {
        let mut bridge = audio.0.lock().await;
        let stop_handle = match bridge.start(write_tx.clone()) {
            Ok(h) => h,
            Err(e) => {
                write_handle.abort();
                emit_session_status(&app, &seq.0, "error", Some("audio device unavailable"));
                return Err(format!("audio bridge: {e}"));
            }
        };
        // Grab the running flag *after* a successful start so the poll in the
        // read loop sees the flag set by the cpal error_callback.
        let running = bridge.running_flag();
        (running, stop_handle)
    };

    // An `Arc` clone of the inner bridge so the audio_handler closure below can
    // call `handle_server_audio` without holding the Tauri `State` guard across
    // the `'static` boundary that `tokio::spawn` requires.  The bridge is only
    // accessed for playback (immutable `&self`), so there is no contention with
    // the `stop_session` path (which holds the `Mutex` for a brief `stop()` call).
    let audio_arc = Arc::clone(&audio.0);
    let audio_handler = move |event: &serde_json::Value| {
        // `try_lock` is non-blocking; if `stop_session` is racing to stop the
        // bridge we simply skip one audio chunk rather than blocking the read loop.
        if let Ok(bridge) = audio_arc.try_lock() {
            bridge.handle_server_audio(event);
        }
    };

    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let cost = Arc::new(TokioMutex::new(tracker));
    let recorder_arc = Arc::clone(&recorder.0);
    let dispatcher_arc = Arc::clone(&dispatcher.0);
    let app_for_loop = app.clone();
    let seq_for_loop = Arc::clone(&seq.0);
    let emit = move |state: &str, error: Option<&str>| {
        emit_session_status(&app_for_loop, &seq_for_loop, state, error);
    };
    let session_for_loop = Arc::clone(&session.0);

    // `stop_audio` is called by run_read_loop on EVERY exit path (error /
    // budget / timeout / shutdown / normal close) so the audio bridge is always
    // torn down fail-closed even when stop_session was not called explicitly.
    //
    // P0 fix: uses the lock-free `AudioStopHandle` captured at start() time.
    // The closure does ONLY: running.store(false) + try_send(FlushThenStop or StopNow).
    // It never calls `try_lock()`, so it CANNOT silently skip the mic stop under
    // contention.
    //
    // P1 fix: the bool arg selects graceful (true → FlushThenStop, unconditional flush)
    // vs immediate (false → StopNow, discard tail). Abnormal exits pass false so no
    // tail PCM races onto the WS after the writer is aborted.
    let stop_audio = move |graceful: bool| {
        if graceful {
            stop_handle.stop_graceful();
        } else {
            stop_handle.stop_immediate();
        }
    };

    // P1 fix: extract the AbortHandle BEFORE storing write_handle in ActiveSession.
    // The read loop aborts the writer on abnormal exits via this handle so
    // already-queued PCM is not flushed after a budget trip / WS error / timeout.
    // On normal server-close exits the writer is left to drain gracefully.
    let writer_abort_handle = write_handle.abort_handle();

    // Live cost emitter (koe-9xi): on each usage frame the read loop calls this with
    // the authoritative (effective month, cross-session total, budget); we mint a
    // shared sequence, build the single `CostSnapshot` DTO, and push it on the
    // `cost-update` channel for the UI's live header. The payload is numbers + a bool
    // only — no key / path / PII ever reaches the WebView (cf. session-status). Owns
    // its own `AppHandle` + sequence clones so the spawned loop is `'static`.
    let app_for_cost = app.clone();
    let seq_for_cost = Arc::clone(&seq.0);
    let emit_cost = move |month: u32, used: u64, budget: BudgetConfig| {
        use tauri::Emitter;
        let snapshot = CostSnapshot::new(month, used, &budget, seq_for_cost.next());
        let _ = app_for_cost.emit("cost-update", snapshot);
    };

    // Pre-tool thinking emitter (glass-box M1, koe-sua.1): when a function call
    // arrives the read loop calls this with (call_id, tool_name) BEFORE it spawns
    // the dispatch, so this `thinking-event` always precedes the dispatcher's
    // `tool-event` phase=start (and shares the SAME `seq` counter, so a disclosure
    // sequences below the start it precedes). The payload is built from the tool
    // NAME only — `ThinkingEvent::for_tool` redacts/bounds it — so no key / path /
    // PII / tool argument ever reaches the WebView. Owns its own `AppHandle` +
    // sequence clones so the spawned loop stays `'static`.
    let app_for_thinking = app.clone();
    let seq_for_thinking = Arc::clone(&seq.0);
    let emit_thinking = move |call_id: &str, tool_name: &str| {
        use tauri::Emitter;
        let payload = ThinkingEvent::for_tool(call_id, tool_name, seq_for_thinking.next());
        let _ = app_for_thinking.emit("thinking-event", payload);
    };

    // Detached: the loop clears the session slot + emits idle on its own exit;
    // stop_session signals it via shutdown_tx rather than holding its handle.
    // `generation` was minted above (right after the is_some check) and
    // `latest_generation` is the shared counter the loop checks at finalize.
    tokio::spawn(run_read_loop(
        stream,
        provider,
        write_tx,
        cost,
        recorder_arc,
        dispatcher_arc,
        shutdown_rx,
        emit,
        session_for_loop,
        generation,
        latest_generation,
        audio_handler,
        mic_running,
        stop_audio,
        Some(writer_abort_handle),
        emit_cost,
        emit_thinking,
    ));

    *guard = Some(ActiveSession {
        generation,
        shutdown_tx,
        write_handle,
    });
    Ok(())
}

/// Stops the active session (idempotent). Signals shutdown, aborts the read loop
/// (which aborts in-flight dispatches) and the write task (dropping the receiver).
#[tauri::command]
pub async fn stop_session(
    session: tauri::State<'_, ManagedSession>,
    audio: tauri::State<'_, ManagedAudioBridge>,
) -> Result<(), String> {
    let taken = { session.0.lock().await.take() };
    if let Some(active) = taken {
        // Signal the read loop to break; it clears the (now-empty) slot and emits
        // the single terminal idle on exit. Abort the writer so it stops promptly.
        // We do NOT abort read_handle — letting it run its shutdown arm guarantees
        // the in-flight dispatch cleanup + the one idle emission happen exactly once.
        let _ = active.shutdown_tx.send(());
        // Abort the writer FIRST (before stopping audio) so no tail PCM that might
        // still be in-flight can reach the WS after manual shutdown.
        active.write_handle.abort();
    }
    // Stop the audio bridge immediately (no tail flush) — manual shutdown aborts
    // the writer first so no tail PCM should race onto the WS.  Idempotent: safe
    // even if start() was never called.
    audio.0.lock().await.stop_immediate();
    Ok(())
}

/// Builds a [`CostSnapshot`] from the recorder's authoritative monthly total and a
/// budget (koe-9xi). The spend is the additive ledger value for `month`
/// ([`RecorderAdapter::load_cost_snapshot`]); an absent row (no usage yet this
/// month) is `0` spent — NOT an error. A recorder failure is propagated as `Err`
/// (fail-closed): the caller surfaces an explicit "unknown" state rather than a
/// fabricated $0 that would hide real spend. Pure + sync so it is unit-testable
/// with a recorder double (no Tauri runtime); the command wraps it in
/// `spawn_blocking`.
fn build_cost_snapshot(
    recorder: &dyn RecorderAdapter,
    budget: &BudgetConfig,
    month: u32,
    sequence: u64,
) -> Result<CostSnapshot, String> {
    let used = recorder
        .load_cost_snapshot(month)
        .map_err(|e| e.to_string())?
        .unwrap_or(0);
    Ok(CostSnapshot::new(month, used, budget, sequence))
}

/// Returns the current month's cost snapshot for the UI's live header (koe-9xi) —
/// the **pull** path (the matching **push** is the `cost-update` emit in the read
/// loop). The spend comes from the recorder's additive ledger (the cross-session
/// authority, not a session-local total), the cap from the persisted
/// [`BudgetConfig`]; `over_budget` is decided in u64 (`is_over`), never recomputed
/// from the display f64. A shared sequence is minted so the frontend can drop this
/// snapshot if a newer push already arrived (and vice-versa). Fail-closed: a
/// settings- or recorder-load failure returns `Err` (the UI shows "unknown"), never
/// a fabricated $0 / unlimited. Contains no secret / PII (numbers + a bool only).
#[tauri::command]
pub async fn get_cost_snapshot(
    settings: tauri::State<'_, ManagedSettings>,
    recorder: tauri::State<'_, ManagedRecorder>,
    seq: tauri::State<'_, ManagedSequenceCounter>,
) -> Result<CostSnapshot, String> {
    // Settings is a small JSON read (mirrors start_session's direct load); the
    // blocking SQLite read is wrapped in spawn_blocking below.
    let budget = settings.0.load().map_err(|e| e.to_string())?.budget;
    let month = current_yyyymm();
    let sequence = seq.0.next();
    let rec = Arc::clone(&recorder.0);
    tokio::task::spawn_blocking(move || build_cost_snapshot(rec.as_ref(), &budget, month, sequence))
        .await
        .map_err(|_| "cost snapshot unavailable".to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    use std::sync::Mutex as StdMutex;

    use base64::Engine as _;
    use crate::cost_tracker::{BudgetConfig, NANODOLLARS_PER_USD};
    use crate::realtime_provider::OpenAiRealtime;
    use crate::realtime_types::{DispatchResult, NoopDispatcher, ToolSchema};
    use crate::storage::adapter::{ConversationEvent, Note, RecorderError};

    /// Generation passed to `run_read_loop` in tests whose `session` slot starts
    /// empty (`None`): paired with [`test_counter`] so the `None` arm sees "I am
    /// still the latest start" and emits as before. Tests that exercise the
    /// generation guard pass explicit `GEN_*` values instead (see the koe-ego
    /// handover tests).
    const TEST_GENERATION: u64 = 1;

    /// The `latest_generation` counter for a loop running [`TEST_GENERATION`] with
    /// no newer start: `TEST_GENERATION + 1` is exactly the value `start_session`
    /// leaves after minting `TEST_GENERATION`, so the `None` arm treats this loop
    /// as the latest start and still emits its terminal status.
    fn test_counter() -> Arc<AtomicU64> {
        Arc::new(AtomicU64::new(TEST_GENERATION + 1))
    }

    /// Builds an `ActiveSession` standing in for a session that occupies the slot,
    /// with `generation` set. The `shutdown_tx` / `write_handle` are inert (a
    /// dropped receiver, an immediately-finished task) — the koe-ego tests only
    /// inspect `generation` to prove the slot was (not) cleared.
    fn fake_active_session(generation: u64) -> ActiveSession {
        let (shutdown_tx, _shutdown_rx) = oneshot::channel();
        let write_handle = tokio::spawn(async {});
        ActiveSession { generation, shutdown_tx, write_handle }
    }

    // ---- current_yyyymm ------------------------------------------------------

    #[test]
    fn current_yyyymm_is_a_plausible_yyyymm() {
        let ym = current_yyyymm();
        let year = ym / 100;
        let month = ym % 100;
        assert!((2024..=2100).contains(&year), "year {year}");
        assert!((1..=12).contains(&month), "month {month}");
    }

    // ---- read loop: frame injection ------------------------------------------

    /// Records dispatch calls and returns a fixed result.
    struct RecordingDispatcher {
        calls: StdMutex<Vec<String>>,
    }
    impl DispatcherSeam for RecordingDispatcher {
        fn dispatch(
            &self,
            call: FunctionCall,
        ) -> crate::realtime_types::BoxFuture<'static, DispatchResult> {
            self.calls.lock().unwrap().push(call.name.clone());
            Box::pin(async move {
                crate::realtime_types::function_call_output(&call.call_id, "{\"ok\":true}".into())
            })
        }
        fn tool_schemas(&self) -> Vec<ToolSchema> {
            Vec::new()
        }
    }

    /// Minimal recorder double; conversation logging + cost-snapshot saves
    /// succeed (so neither stops the loop), the rest is unused.
    struct OkRecorder;
    impl RecorderAdapter for OkRecorder {
        fn save_note(&self, _t: &str) -> Result<i64, RecorderError> {
            unimplemented!()
        }
        fn list_recent_notes(&self, _l: u32) -> Result<Vec<Note>, RecorderError> {
            unimplemented!()
        }
        fn log_conversation_event(&self, _r: &str, _k: &str, _s: &str) -> Result<i64, RecorderError> {
            Ok(0)
        }
        fn list_recent_events(&self, _l: u32) -> Result<Vec<ConversationEvent>, RecorderError> {
            unimplemented!()
        }
        fn add_month_cost(&self, _m: u32, n: u64) -> Result<u64, RecorderError> {
            // No prior state → merged total == the written total (echo). Mirrors
            // the real adapter's monotonic-merge return for a single-session test.
            Ok(n)
        }
        fn load_cost_snapshot(&self, _m: u32) -> Result<Option<u64>, RecorderError> {
            Ok(None)
        }
        fn health_check(&self) -> Result<(), RecorderError> {
            Ok(())
        }
    }

    /// Captured `(role, kind, summary)` tuples shared with a test after a run.
    type RecordedEvents = Arc<StdMutex<Vec<(String, String, String)>>>;

    /// Recorder double that captures every `log_conversation_event` call in
    /// order (koe-emd) so a test can assert the journalled sequence + role/kind.
    /// Cost-snapshot saves succeed so the cost path never stops the loop.
    struct RecordingRecorder {
        events: RecordedEvents,
    }
    impl RecordingRecorder {
        fn new() -> (Self, RecordedEvents) {
            let events = RecordedEvents::default();
            (
                Self {
                    events: Arc::clone(&events),
                },
                events,
            )
        }
    }
    impl RecorderAdapter for RecordingRecorder {
        fn save_note(&self, _t: &str) -> Result<i64, RecorderError> {
            unimplemented!()
        }
        fn list_recent_notes(&self, _l: u32) -> Result<Vec<Note>, RecorderError> {
            unimplemented!()
        }
        fn log_conversation_event(&self, role: &str, kind: &str, summary: &str) -> Result<i64, RecorderError> {
            let mut e = self.events.lock().unwrap();
            e.push((role.to_string(), kind.to_string(), summary.to_string()));
            Ok(e.len() as i64)
        }
        fn list_recent_events(&self, _l: u32) -> Result<Vec<ConversationEvent>, RecorderError> {
            unimplemented!()
        }
        fn add_month_cost(&self, _m: u32, n: u64) -> Result<u64, RecorderError> {
            Ok(n)
        }
        fn load_cost_snapshot(&self, _m: u32) -> Result<Option<u64>, RecorderError> {
            Ok(None)
        }
        fn health_check(&self) -> Result<(), RecorderError> {
            Ok(())
        }
    }

    fn frame_stream(
        frames: Vec<Value>,
    ) -> impl Stream<Item = Result<Message, WsError>> + Unpin {
        futures_util::stream::iter(
            frames
                .into_iter()
                .map(|v| Ok(Message::Text(v.to_string().into())))
                .collect::<Vec<_>>(),
        )
    }

    fn collect_emit() -> (Arc<StdMutex<Vec<(String, Option<String>)>>>, impl Fn(&str, Option<&str>)) {
        let log = Arc::new(StdMutex::new(Vec::new()));
        let l = Arc::clone(&log);
        let emit = move |state: &str, err: Option<&str>| {
            l.lock().unwrap().push((state.to_string(), err.map(str::to_string)));
        };
        (log, emit)
    }

    // ---- koe-ego: stop_session -> start_session slot-handover generation guard --

    #[tokio::test]
    async fn handover_race_exiting_loop_keeps_newer_session_slot() {
        // The real stop_session->start_session handover, modeled with a strict
        // happens-before so it is deterministic (no flaky timing):
        //   1. session A (GEN_A) is live and its read loop is parked;
        //   2. stop_session takes A out of the slot (slot now empty);
        //   3. start_session(B) stores a NEWER session (GEN_B) in the slot;
        //   4. ONLY THEN is A's loop signaled, so its teardown runs *after* B is in
        //      the slot — exactly the dangerous interleaving.
        // A's exiting loop must leave B's handle intact and emit no terminal idle
        // (else B's live WS would be orphaned/unstoppable → BYOK double-charge, and
        // the UI would flash to idle while B is connected).
        const GEN_A: u64 = 1;
        const GEN_B: u64 = 2;

        let slot: Arc<TokioMutex<Option<ActiveSession>>> = Arc::new(TokioMutex::new(None));
        let (log, emit) = collect_emit();

        // (1) A is live: store its handle and start its read loop, parked on a
        // stream that never yields (only the shutdown signal will end it).
        let (a_shutdown_tx, a_shutdown_rx) = oneshot::channel();
        let a_write = tokio::spawn(async {});
        *slot.lock().await = Some(ActiveSession {
            generation: GEN_A,
            shutdown_tx: a_shutdown_tx,
            write_handle: a_write,
        });

        let cost = Arc::new(TokioMutex::new(CostTracker::new(
            BudgetConfig { enabled: false, monthly_limit_nanodollars: 0 },
            current_yyyymm(),
        )));
        let (a_write_tx, _a_write_rx) = mpsc::channel::<Message>(8);
        let a_loop = tokio::spawn(run_read_loop(
            futures_util::stream::pending::<Result<Message, WsError>>(),
            Arc::new(OpenAiRealtime::new()) as Arc<dyn RealtimeProvider>,
            a_write_tx,
            cost,
            Arc::new(OkRecorder) as Arc<dyn RecorderAdapter>,
            Arc::new(NoopDispatcher) as Arc<dyn DispatcherSeam>,
            a_shutdown_rx,
            emit,
            Arc::clone(&slot),
            GEN_A,
            Arc::new(AtomicU64::new(GEN_B + 1)), // a newer start (B) exists
            |_event: &serde_json::Value| {},
            Arc::new(AtomicBool::new(true)),
            |_graceful: bool| {},
            None,
        |_month: u32, _used: u64, _budget: BudgetConfig| {},
        |_call_id: &str, _tool: &str| {},));

        // Let A's loop reach its select and park on the pending stream.
        tokio::task::yield_now().await;

        // (2) stop_session(A): take A's handle out of the slot (slot now empty).
        let a_active = slot.lock().await.take().expect("A occupies the slot");
        assert_eq!(a_active.generation, GEN_A);

        // (3) start_session(B): a NEWER session takes the now-empty slot, BEFORE
        // A's exiting loop reaches its terminal slot-clear.
        *slot.lock().await = Some(fake_active_session(GEN_B));

        // (4) Now signal A's loop to exit; its teardown runs with B in the slot.
        let _ = a_active.shutdown_tx.send(());
        a_loop.await.expect("A read loop joins");

        assert_eq!(
            slot.lock().await.as_ref().map(|s| s.generation),
            Some(GEN_B),
            "exiting A loop must not clear B's (newer) slot during a stop->start handover (koe-ego)"
        );
        let emitted: Vec<_> = log.lock().unwrap().clone();
        assert!(
            !emitted.iter().any(|(s, _)| s == "idle"),
            "exiting A loop must not emit idle while a newer session owns the slot: {emitted:?}"
        );
    }

    #[tokio::test]
    async fn exiting_loop_on_close_does_not_clear_newer_generation() {
        // Terminal slot-clear site (normal server-close exit): if the slot already
        // holds a newer generation (a handover completed before this loop's
        // teardown), the exiting loop must leave it and emit no idle.
        const GEN_A: u64 = 1;
        const GEN_B: u64 = 2;
        let slot: Arc<TokioMutex<Option<ActiveSession>>> = Arc::new(TokioMutex::new(None));
        *slot.lock().await = Some(fake_active_session(GEN_B));

        let cost = Arc::new(TokioMutex::new(CostTracker::new(
            BudgetConfig { enabled: false, monthly_limit_nanodollars: 0 },
            current_yyyymm(),
        )));
        let (write_tx, _write_rx) = mpsc::channel::<Message>(8);
        let (_sd_tx, sd_rx) = oneshot::channel();
        let (log, emit) = collect_emit();

        run_read_loop(
            frame_stream(vec![]), // empty stream → immediate normal (server-close) exit
            Arc::new(OpenAiRealtime::new()) as Arc<dyn RealtimeProvider>,
            write_tx,
            cost,
            Arc::new(OkRecorder) as Arc<dyn RecorderAdapter>,
            Arc::new(NoopDispatcher) as Arc<dyn DispatcherSeam>,
            sd_rx,
            emit,
            Arc::clone(&slot),
            GEN_A,
            Arc::new(AtomicU64::new(GEN_B + 1)), // a newer start (B) exists
            |_| {},
            Arc::new(AtomicBool::new(true)),
            |_| {},
            None,
        |_month: u32, _used: u64, _budget: BudgetConfig| {},
        |_call_id: &str, _tool: &str| {},)
        .await;

        assert_eq!(
            slot.lock().await.as_ref().map(|s| s.generation),
            Some(GEN_B),
            "old loop must not clear a newer session's slot on close (koe-ego)"
        );
        let emitted: Vec<_> = log.lock().unwrap().clone();
        assert!(
            !emitted.iter().any(|(s, _)| s == "idle"),
            "old loop must not emit idle while a newer session owns the slot: {emitted:?}"
        );
    }

    #[tokio::test]
    async fn mic_lost_preloop_does_not_clear_newer_generation() {
        // The pre-loop mic-lost early-return clear site honors the generation
        // guard: with a newer generation in the slot it neither takes that handle
        // NOR emits a terminal status — its `error` is generation-guarded inside
        // finalize_session_slot (koe-ego), so a dying loop can't flash its error
        // over the connected newer session.
        const GEN_A: u64 = 1;
        const GEN_B: u64 = 2;
        let slot: Arc<TokioMutex<Option<ActiveSession>>> = Arc::new(TokioMutex::new(None));
        *slot.lock().await = Some(fake_active_session(GEN_B));

        let cost = Arc::new(TokioMutex::new(CostTracker::new(
            BudgetConfig { enabled: false, monthly_limit_nanodollars: 0 },
            current_yyyymm(),
        )));
        let (write_tx, _write_rx) = mpsc::channel::<Message>(8);
        let (_sd_tx, sd_rx) = oneshot::channel();
        let (log, emit) = collect_emit();

        run_read_loop(
            frame_stream(vec![]),
            Arc::new(OpenAiRealtime::new()) as Arc<dyn RealtimeProvider>,
            write_tx,
            cost,
            Arc::new(OkRecorder) as Arc<dyn RecorderAdapter>,
            Arc::new(NoopDispatcher) as Arc<dyn DispatcherSeam>,
            sd_rx,
            emit,
            Arc::clone(&slot),
            GEN_A,
            Arc::new(AtomicU64::new(GEN_B + 1)), // a newer start (B) exists
            |_| {},
            Arc::new(AtomicBool::new(false)), // mic NOT running → pre-loop early return
            |_| {},
            None,
        |_month: u32, _used: u64, _budget: BudgetConfig| {},
        |_call_id: &str, _tool: &str| {},)
        .await;

        assert_eq!(
            slot.lock().await.as_ref().map(|s| s.generation),
            Some(GEN_B),
            "mic-lost early return must not clear a newer session's slot (koe-ego)"
        );
        // With GEN_B owning the slot, the dying loop emits NO terminal status —
        // neither its `error` nor an `idle` — over the newer session.
        let emitted: Vec<_> = log.lock().unwrap().clone();
        assert!(
            emitted.is_empty(),
            "mic-lost early return must emit no terminal status over a newer session: {emitted:?}"
        );
    }

    #[tokio::test]
    async fn handover_error_exit_does_not_emit_over_newer_session() {
        // The in-loop error arms (here: a transport error → the connection-error
        // arm) also route their terminal `error` through finalize_session_slot, so
        // a stop->start handover that put a newer generation in the slot must NOT
        // flash the dying loop's `error` over the connected newer session (koe-ego).
        // Otherwise the stale error sticks in the UI and disables the stop control
        // for a live, BYOK-billing session.
        const GEN_A: u64 = 1;
        const GEN_B: u64 = 2;
        let slot: Arc<TokioMutex<Option<ActiveSession>>> = Arc::new(TokioMutex::new(None));
        *slot.lock().await = Some(fake_active_session(GEN_B));

        let cost = Arc::new(TokioMutex::new(CostTracker::new(
            BudgetConfig { enabled: false, monthly_limit_nanodollars: 0 },
            current_yyyymm(),
        )));
        let (write_tx, _write_rx) = mpsc::channel::<Message>(8);
        let (_sd_tx, sd_rx) = oneshot::channel();
        let (log, emit) = collect_emit();

        run_read_loop(
            futures_util::stream::iter(vec![Err(WsError::ConnectionClosed)]),
            Arc::new(OpenAiRealtime::new()) as Arc<dyn RealtimeProvider>,
            write_tx,
            cost,
            Arc::new(OkRecorder) as Arc<dyn RecorderAdapter>,
            Arc::new(NoopDispatcher) as Arc<dyn DispatcherSeam>,
            sd_rx,
            emit,
            Arc::clone(&slot),
            GEN_A,
            Arc::new(AtomicU64::new(GEN_B + 1)), // a newer start (B) exists
            |_| {},
            Arc::new(AtomicBool::new(true)),
            |_| {},
            None,
        |_month: u32, _used: u64, _budget: BudgetConfig| {},
        |_call_id: &str, _tool: &str| {},)
        .await;

        assert_eq!(
            slot.lock().await.as_ref().map(|s| s.generation),
            Some(GEN_B),
            "in-loop error exit must not clear a newer session's slot (koe-ego)"
        );
        let emitted: Vec<_> = log.lock().unwrap().clone();
        assert!(
            emitted.is_empty(),
            "in-loop error exit must emit no terminal status over a newer session: {emitted:?}"
        );
    }

    #[tokio::test]
    async fn exiting_loop_clears_its_own_generation_and_emits_idle() {
        // The fix must not regress the normal path: a loop whose generation still
        // owns the slot clears it and emits the single terminal idle on a clean
        // exit.
        const GEN: u64 = 7;
        let slot: Arc<TokioMutex<Option<ActiveSession>>> = Arc::new(TokioMutex::new(None));
        *slot.lock().await = Some(fake_active_session(GEN));

        let cost = Arc::new(TokioMutex::new(CostTracker::new(
            BudgetConfig { enabled: false, monthly_limit_nanodollars: 0 },
            current_yyyymm(),
        )));
        let (write_tx, _write_rx) = mpsc::channel::<Message>(8);
        let (_sd_tx, sd_rx) = oneshot::channel();
        let (log, emit) = collect_emit();

        run_read_loop(
            frame_stream(vec![]),
            Arc::new(OpenAiRealtime::new()) as Arc<dyn RealtimeProvider>,
            write_tx,
            cost,
            Arc::new(OkRecorder) as Arc<dyn RecorderAdapter>,
            Arc::new(NoopDispatcher) as Arc<dyn DispatcherSeam>,
            sd_rx,
            emit,
            Arc::clone(&slot),
            GEN,
            Arc::new(AtomicU64::new(GEN + 1)), // this loop is the latest start
            |_| {},
            Arc::new(AtomicBool::new(true)),
            |_| {},
            None,
        |_month: u32, _used: u64, _budget: BudgetConfig| {},
        |_call_id: &str, _tool: &str| {},)
        .await;

        assert!(
            slot.lock().await.is_none(),
            "own-generation slot must be cleared on clean exit (koe-ego)"
        );
        let emitted: Vec<_> = log.lock().unwrap().clone();
        assert!(
            emitted.iter().any(|(s, _)| s == "idle"),
            "clean exit must emit the terminal idle: {emitted:?}"
        );
    }

    #[tokio::test]
    async fn none_arm_stays_silent_when_a_newer_start_has_begun() {
        // koe-ego None-arm case (Codex MCP + Codex Cloud R-C): after stop_session
        // takes our handle (slot None), a NEWER start_session can begin and even
        // FAIL before storing an ActiveSession (connect/setup/audio error) — the
        // slot stays None but the generation counter has advanced past us. The old
        // loop's terminal slot-clear must then stay silent, so its `idle` cannot
        // land over the newer (failed) start's `error` (else a failed reconnect
        // during a restart would be wrongly cleared to idle).
        const GEN_A: u64 = 1;
        let slot: Arc<TokioMutex<Option<ActiveSession>>> = Arc::new(TokioMutex::new(None));
        // Counter past GEN_A + 1: a newer start (generation GEN_A + 1) has begun,
        // even though it left nothing in the slot (it failed before storing).
        let counter = Arc::new(AtomicU64::new(GEN_A + 2));

        let cost = Arc::new(TokioMutex::new(CostTracker::new(
            BudgetConfig { enabled: false, monthly_limit_nanodollars: 0 },
            current_yyyymm(),
        )));
        let (write_tx, _write_rx) = mpsc::channel::<Message>(8);
        let (_sd_tx, sd_rx) = oneshot::channel();
        let (log, emit) = collect_emit();

        run_read_loop(
            frame_stream(vec![]), // clean server-close exit (terminal_error = None → would-be idle)
            Arc::new(OpenAiRealtime::new()) as Arc<dyn RealtimeProvider>,
            write_tx,
            cost,
            Arc::new(OkRecorder) as Arc<dyn RecorderAdapter>,
            Arc::new(NoopDispatcher) as Arc<dyn DispatcherSeam>,
            sd_rx,
            emit,
            Arc::clone(&slot),
            GEN_A,
            Arc::clone(&counter),
            |_| {},
            Arc::new(AtomicBool::new(true)),
            |_| {},
            None,
        |_month: u32, _used: u64, _budget: BudgetConfig| {},
        |_call_id: &str, _tool: &str| {},)
        .await;

        let emitted: Vec<_> = log.lock().unwrap().clone();
        assert!(
            !emitted.iter().any(|(s, _)| s == "idle"),
            "None-arm must stay silent when a newer start has begun: {emitted:?}"
        );
    }

    #[tokio::test]
    async fn function_call_frame_is_dispatched_and_result_sent() {
        let disp = Arc::new(RecordingDispatcher { calls: StdMutex::new(Vec::new()) });
        let cost = Arc::new(TokioMutex::new(CostTracker::new(
            BudgetConfig { enabled: false, monthly_limit_nanodollars: 0 },
            current_yyyymm(),
        )));
        let (write_tx, mut write_rx) = mpsc::channel::<Message>(8);
        let (_sd_tx, sd_rx) = oneshot::channel();
        let (_log, emit) = collect_emit();

        let frames = vec![serde_json::json!({
            "type": "response.function_call_arguments.done",
            "call_id": "call_1",
            "name": "write_note",
            "arguments": "{\"text\":\"hi\"}"
        })];
        run_read_loop(
            frame_stream(frames),
            Arc::new(OpenAiRealtime::new()) as Arc<dyn RealtimeProvider>,
            write_tx,
            cost,
            Arc::new(OkRecorder) as Arc<dyn RecorderAdapter>,
            disp.clone() as Arc<dyn DispatcherSeam>,
            sd_rx,
            emit,
            Arc::new(TokioMutex::new(None)),
            TEST_GENERATION,
            test_counter(),
            |_| {}, // no-op audio_handler (no device in test)
            Arc::new(AtomicBool::new(true)), // mic always running in tests
            |_| {}, // no-op stop_audio (no device in test)
            None, // no write task to abort in unit tests
            |_month: u32, _used: u64, _budget: BudgetConfig| {},
            |_call_id: &str, _tool: &str| {},
        )
        .await;

        assert_eq!(disp.calls.lock().unwrap().as_slice(), ["write_note"]);
        // The dispatch task sends two frames (item.create + response.create).
        let f1 = write_rx.recv().await.expect("item.create frame");
        let f2 = write_rx.recv().await.expect("response.create frame");
        assert!(matches!(f1, Message::Text(_)));
        assert!(matches!(f2, Message::Text(_)));
    }

    // ---- thinking-event disclosure (glass-box M1, koe-sua.1) ------------------

    #[tokio::test]
    async fn thinking_event_emitted_before_tool_dispatch() {
        // The glass-box M1 ordering invariant: when a function call arrives, the
        // read loop discloses (`emit_thinking`) the imminent action BEFORE it
        // dispatches the tool — i.e. before the dispatcher emits that call's
        // `tool-event` phase=start. We prove the order with a SINGLE shared log:
        // the thinking emitter appends "think:…" and a dispatcher double appends
        // "dispatch:…" when it runs, so the recorded order IS the real order. The
        // disclosure is built from the tool NAME only, so the request's "secret"
        // argument must never reach what the closure sees (redaction).
        let order: Arc<StdMutex<Vec<String>>> = Arc::new(StdMutex::new(Vec::new()));

        // A dispatcher double that records its run into the shared order log.
        struct OrderDispatcher(Arc<StdMutex<Vec<String>>>);
        impl DispatcherSeam for OrderDispatcher {
            fn dispatch(
                &self,
                call: FunctionCall,
            ) -> crate::realtime_types::BoxFuture<'static, crate::realtime_types::DispatchResult>
            {
                let log = Arc::clone(&self.0);
                Box::pin(async move {
                    log.lock().unwrap().push(format!("dispatch:{}", call.name));
                    crate::realtime_types::function_call_output(&call.call_id, "{\"ok\":true}".into())
                })
            }
            fn tool_schemas(&self) -> Vec<crate::realtime_types::ToolSchema> {
                Vec::new()
            }
        }

        let disp = Arc::new(OrderDispatcher(Arc::clone(&order)));
        let cost = Arc::new(TokioMutex::new(CostTracker::new(
            BudgetConfig { enabled: false, monthly_limit_nanodollars: 0 },
            current_yyyymm(),
        )));
        let (write_tx, _write_rx) = mpsc::channel::<Message>(8);
        let (_sd_tx, sd_rx) = oneshot::channel();
        let (_log, emit) = collect_emit();

        // Recording emit_thinking: append "think:<call_id>:<tool>" to the SAME
        // order log, and stash the raw (call_id, tool) the closure received so we
        // can assert no argument leaked into what the disclosure is built from.
        let seen: Arc<StdMutex<Vec<(String, String)>>> = Arc::new(StdMutex::new(Vec::new()));
        let order_for_think = Arc::clone(&order);
        let seen_for_think = Arc::clone(&seen);
        let emit_thinking = move |call_id: &str, tool: &str| {
            order_for_think
                .lock()
                .unwrap()
                .push(format!("think:{call_id}:{tool}"));
            seen_for_think
                .lock()
                .unwrap()
                .push((call_id.to_string(), tool.to_string()));
        };

        let frames = vec![serde_json::json!({
            "type": "response.function_call_arguments.done",
            "call_id": "call_1",
            "name": "web_search",
            "arguments": "{\"query\":\"secret\"}"
        })];
        run_read_loop(
            frame_stream(frames),
            Arc::new(OpenAiRealtime::new()) as Arc<dyn RealtimeProvider>,
            write_tx,
            cost,
            Arc::new(OkRecorder) as Arc<dyn RecorderAdapter>,
            disp.clone() as Arc<dyn DispatcherSeam>,
            sd_rx,
            emit,
            Arc::new(TokioMutex::new(None)),
            TEST_GENERATION,
            test_counter(),
            |_| {},
            Arc::new(AtomicBool::new(true)),
            |_| {},
            None,
            |_month: u32, _used: u64, _budget: BudgetConfig| {},
            emit_thinking,
        )
        .await;

        let log = order.lock().unwrap().clone();
        assert_eq!(
            log,
            vec![
                "think:call_1:web_search".to_string(),
                "dispatch:web_search".to_string(),
            ],
            "the thinking disclosure must be emitted BEFORE the tool is dispatched"
        );
        // The closure is handed the tool NAME only — never the arguments — so the
        // request's "secret" query cannot reach the disclosure it builds.
        let seen = seen.lock().unwrap().clone();
        assert_eq!(seen, vec![("call_1".to_string(), "web_search".to_string())]);
        for (call_id, tool) in &seen {
            assert!(
                !call_id.contains("secret") && !tool.contains("secret"),
                "no argument value may reach the thinking closure"
            );
        }
    }

    #[test]
    fn thinking_event_payload_is_redacted_and_camelcased() {
        // The serialized payload matches `ThinkingEvent` in
        // src/features/activity/types.ts (camelCase) and carries only redacted,
        // tool-NAME-derived fields — never a raw-CoT or args field.
        let e = ThinkingEvent::for_tool("call_42", "web_search", 7);
        let v = serde_json::to_value(&e).unwrap();
        assert_eq!(v["eventId"], "think-7");
        assert_eq!(v["actionId"], "call_42");
        assert_eq!(v["sequence"], 7);
        assert_eq!(v["phase"], "deciding");
        assert_eq!(v["plan"], "ウェブを検索しています");
        assert_eq!(v["tool"], "web_search");
        assert_eq!(v["source"], "web");
        // The calibration label (koe-sua.2) is never fabricated in M1.
        assert!(v.get("confidence").is_none());
        assert!(v["timestamp"].is_i64());
    }

    #[test]
    fn thinking_event_unknown_tool_falls_back_safely() {
        // An unknown / new tool name still yields a safe, non-leaking disclosure: a
        // generic plan, no source, and the (bounded) tool name — never a panic or a
        // leak. `source` is omitted from the JSON when None.
        let e = ThinkingEvent::for_tool("c", "totally_new_tool", 1);
        assert_eq!(e.plan, "ツールを使おうとしています");
        assert_eq!(e.source, None);
        assert_eq!(e.tool.as_deref(), Some("totally_new_tool"));
        let v = serde_json::to_value(&e).unwrap();
        assert!(v.get("source").is_none(), "absent source is omitted, not null");
    }

    #[test]
    fn thinking_event_oversized_tool_name_is_bounded() {
        // A hostile oversized tool name cannot bloat the payload: the displayed
        // name is char-bounded to MAX_TOOL_NAME_LEN.
        let huge = "x".repeat(MAX_TOOL_NAME_LEN * 4);
        let e = ThinkingEvent::for_tool("c", &huge, 1);
        assert_eq!(e.tool.as_deref().map(str::chars).map(Iterator::count), Some(MAX_TOOL_NAME_LEN));
    }

    // ---- conversation log wiring (koe-emd) -----------------------------------

    #[tokio::test]
    async fn conversation_turns_are_recorded_in_frame_order() {
        // The core wiring contract: a user-speech transcript, a tool invocation,
        // and an assistant-speech transcript are journalled in frame order with
        // the role/kind the ConversationEvent type expects. Records flow through a
        // single bounded mpsc (FIFO) via non-blocking try_send on the read loop,
        // drained by one writer task that awaits each write sequentially, so send
        // order == insert order == frame order (list_recent_events orders by row
        // id). The drain on loop exit (drop(rec_tx) + writer.await) flushes the
        // tail, so all three records are persisted before run_read_loop returns.
        //
        // Streaming `.delta` frames are interleaved before each finalized turn to
        // prove exactly-once end to end: deltas map to Ignored, so the recorded
        // vector still contains only the finalized turns (no fragment / no dupe).
        let (rec, events) = RecordingRecorder::new();
        let cost = Arc::new(TokioMutex::new(CostTracker::new(
            BudgetConfig { enabled: false, monthly_limit_nanodollars: 0 },
            current_yyyymm(),
        )));
        // Keep the receiver alive so the dispatch task's sends don't fail-fast.
        let (write_tx, _write_rx) = mpsc::channel::<Message>(8);
        let (_sd_tx, sd_rx) = oneshot::channel();
        let (_log, emit) = collect_emit();

        let frames = vec![
            serde_json::json!({
                "type": "conversation.item.input_audio_transcription.delta",
                "delta": "search the"
            }),
            serde_json::json!({
                "type": "conversation.item.input_audio_transcription.completed",
                "transcript": "search the web for rust"
            }),
            serde_json::json!({
                "type": "response.function_call_arguments.done",
                "call_id": "call_1",
                "name": "web_search",
                "arguments": "{\"q\":\"rust\"}"
            }),
            serde_json::json!({
                "type": "response.output_audio_transcript.delta",
                "delta": "here is"
            }),
            serde_json::json!({
                "type": "response.output_audio_transcript.done",
                "transcript": "here is what I found"
            }),
        ];
        run_read_loop(
            frame_stream(frames),
            Arc::new(OpenAiRealtime::new()) as Arc<dyn RealtimeProvider>,
            write_tx,
            cost,
            Arc::new(rec) as Arc<dyn RecorderAdapter>,
            Arc::new(NoopDispatcher) as Arc<dyn DispatcherSeam>,
            sd_rx,
            emit,
            Arc::new(TokioMutex::new(None)),
            TEST_GENERATION,
            test_counter(),
            |_| {},
            Arc::new(AtomicBool::new(true)),
            |_| {},
            None,
        |_month: u32, _used: u64, _budget: BudgetConfig| {},
        |_call_id: &str, _tool: &str| {},)
        .await;

        let recorded = events.lock().unwrap();
        assert_eq!(
            recorded.as_slice(),
            [
                (
                    "user".to_string(),
                    "speech".to_string(),
                    "search the web for rust".to_string()
                ),
                ("tool".to_string(), "tool".to_string(), "web_search".to_string()),
                (
                    "assistant".to_string(),
                    "speech".to_string(),
                    "here is what I found".to_string()
                ),
            ]
        );
    }

    #[tokio::test]
    async fn asr_completed_records_user_turn_once_and_meters_asr_once() {
        // koe-pbe end to end: enabling input_audio_transcription makes the server
        // emit a user `.completed` frame carrying BOTH the transcript AND a
        // SEPARATELY-BILLED ASR usage. ONE such frame must journal the user turn
        // EXACTLY once AND meter the ASR cost EXACTLY once through the same
        // add_month_cost / cost-update path — no double-record, no double-count, no
        // second cost channel. A streaming delta first proves exactly-once.
        let (rec, events) = RecordingRecorder::new();
        let month = current_yyyymm();
        let budget = BudgetConfig {
            enabled: true,
            monthly_limit_nanodollars: 100 * NANODOLLARS_PER_USD,
        };
        let cost = Arc::new(TokioMutex::new(CostTracker::new(budget, month)));
        let (write_tx, _write_rx) = mpsc::channel::<Message>(8);
        let (_sd_tx, sd_rx) = oneshot::channel();
        let (_log, emit) = collect_emit();
        let cost_emits: CostEmits = Arc::new(StdMutex::new(Vec::new()));
        let sink = Arc::clone(&cost_emits);

        let frames = vec![
            serde_json::json!({
                "type": "conversation.item.input_audio_transcription.delta",
                "delta": "search the"
            }),
            serde_json::json!({
                "type": "conversation.item.input_audio_transcription.completed",
                "item_id": "item_1",
                "content_index": 0,
                "transcript": "search the web for rust",
                "usage": {
                    "type": "tokens",
                    "total_tokens": 22,
                    "input_tokens": 13,
                    "input_token_details": { "text_tokens": 0, "audio_tokens": 13 },
                    "output_tokens": 9
                }
            }),
        ];
        run_read_loop(
            frame_stream(frames),
            Arc::new(OpenAiRealtime::new()) as Arc<dyn RealtimeProvider>,
            write_tx,
            cost,
            Arc::new(rec) as Arc<dyn RecorderAdapter>,
            Arc::new(NoopDispatcher) as Arc<dyn DispatcherSeam>,
            sd_rx,
            emit,
            Arc::new(TokioMutex::new(None)),
            TEST_GENERATION,
            test_counter(),
            |_| {},
            Arc::new(AtomicBool::new(true)),
            |_| {},
            None,
            move |m: u32, u: u64, b: BudgetConfig| sink.lock().unwrap().push((m, u, b)),
            |_call_id: &str, _tool: &str| {},
        )
        .await;

        // (a) the user turn is journalled EXACTLY once (delta Ignored, no dupe).
        assert_eq!(
            events.lock().unwrap().as_slice(),
            [(
                "user".to_string(),
                "speech".to_string(),
                "search the web for rust".to_string()
            )]
        );
        // (b) the ASR usage is metered EXACTLY once via the cost path; the
        //     conservative mapping bills 13 audio-input + 9 text-output tokens at
        //     the realtime rates (over-count vs real ASR pricing = fail-closed).
        let calls = cost_emits.lock().unwrap().clone();
        assert_eq!(calls.len(), 1, "exactly one cost-update for the ASR usage");
        let expected = crate::cost_tracker::Usage {
            audio_input_tokens: 13,
            text_output_tokens: 9,
            ..Default::default()
        }
        .cost_nanodollars();
        assert_eq!(calls[0].0, month);
        assert_eq!(calls[0].1, expected);
        assert!(!CostSnapshot::new(calls[0].0, calls[0].1, &calls[0].2, 0).over_budget);
    }

    #[tokio::test]
    async fn asr_over_budget_records_transcript_then_stops_fail_closed() {
        // The ASR usage on a user `.completed` frame gates the budget like any other
        // usage: an over-cap ASR turn STOPS the session fail-closed. Order matters —
        // the transcript is surfaced FIRST, so the turn is still journalled (it
        // happened) before the gate stops the loop; the over-budget snapshot is
        // emitted before the stop (koe-9xi), and a second frame after the stop is
        // never processed.
        let (rec, events) = RecordingRecorder::new();
        let month = current_yyyymm();
        let budget = BudgetConfig {
            enabled: true,
            monthly_limit_nanodollars: 1, // 1 nanodollar — any ASR usage trips it
        };
        let cost = Arc::new(TokioMutex::new(CostTracker::new(budget, month)));
        let (write_tx, _write_rx) = mpsc::channel::<Message>(8);
        let (_sd_tx, sd_rx) = oneshot::channel();
        let (log, emit) = collect_emit();
        let cost_emits: CostEmits = Arc::new(StdMutex::new(Vec::new()));
        let sink = Arc::clone(&cost_emits);

        let frames = vec![
            serde_json::json!({
                "type": "conversation.item.input_audio_transcription.completed",
                "transcript": "expensive question",
                "usage": {
                    "type": "tokens",
                    "total_tokens": 2000,
                    "input_tokens": 1000,
                    "input_token_details": { "text_tokens": 0, "audio_tokens": 1000 },
                    "output_tokens": 1000
                }
            }),
            // Must NOT be processed after the budget stop short-circuits the loop.
            serde_json::json!({
                "type": "conversation.item.input_audio_transcription.completed",
                "transcript": "should not be recorded"
            }),
        ];
        run_read_loop(
            frame_stream(frames),
            Arc::new(OpenAiRealtime::new()) as Arc<dyn RealtimeProvider>,
            write_tx,
            cost,
            Arc::new(rec) as Arc<dyn RecorderAdapter>,
            Arc::new(NoopDispatcher) as Arc<dyn DispatcherSeam>,
            sd_rx,
            emit,
            Arc::new(TokioMutex::new(None)),
            TEST_GENERATION,
            test_counter(),
            |_| {},
            Arc::new(AtomicBool::new(true)),
            |_| {},
            None,
            move |m: u32, u: u64, b: BudgetConfig| sink.lock().unwrap().push((m, u, b)),
            |_call_id: &str, _tool: &str| {},
        )
        .await;

        // The over-budget turn was still journalled (transcript surfaced before the
        // gate); the post-stop frame is not.
        assert_eq!(
            events.lock().unwrap().as_slice(),
            [(
                "user".to_string(),
                "speech".to_string(),
                "expensive question".to_string()
            )],
            "the over-budget turn is recorded; the post-stop frame is not"
        );
        // The over-budget cost snapshot was emitted before the stop (durable add).
        let calls = cost_emits.lock().unwrap().clone();
        assert_eq!(calls.len(), 1);
        assert!(CostSnapshot::new(calls[0].0, calls[0].1, &calls[0].2, 0).over_budget);
        // The session terminated with the fail-closed budget error.
        let statuses = log.lock().unwrap().clone();
        assert!(
            statuses
                .iter()
                .any(|(s, r)| s == "error" && r.as_deref() == Some("monthly budget exceeded")),
            "session must stop fail-closed on the over-budget ASR usage; got {statuses:?}"
        );
    }

    #[tokio::test]
    async fn inflight_dispatch_count_is_bounded() {
        // koe-wj2 DoS guard: a hostile / compromised model that streams
        // `function_call` frames for the whole session must NOT grow the dispatch
        // JoinSet without bound. We feed MAX_INFLIGHT_DISPATCHES + extra
        // function-call frames; only the cap many may ever be spawned, the rest
        // are skipped (the loop keeps running and exits cleanly — no crash).
        //
        // Determinism rests on the default current-thread test runtime: the read
        // loop never awaits between back-to-back ready frames, so spawned dispatch
        // tasks are NOT polled (hence not reaped) during the burst — exactly the
        // worst case the cap defends against. `RecordingDispatcher` completes
        // immediately, so the normal-exit drain (`join_next().await`) finishes
        // instead of hanging; it records one call per task as the drain polls it,
        // giving a count equal to how many tasks were actually spawned.
        let disp = Arc::new(RecordingDispatcher { calls: StdMutex::new(Vec::new()) });
        let cost = Arc::new(TokioMutex::new(CostTracker::new(
            BudgetConfig { enabled: false, monthly_limit_nanodollars: 0 },
            current_yyyymm(),
        )));
        // Drop the receiver so each task's two result `send`s fail instantly
        // (the tasks ignore the error) rather than blocking on a never-read,
        // bounded channel — which would deadlock the drain. This test only counts
        // dispatches; frame-content correctness is covered by
        // function_call_frame_is_dispatched_and_result_sent.
        let (write_tx, write_rx) = mpsc::channel::<Message>(8);
        drop(write_rx);
        let (_sd_tx, sd_rx) = oneshot::channel();
        let (_log, emit) = collect_emit();

        let extra = 10;
        let frames: Vec<Value> = (0..MAX_INFLIGHT_DISPATCHES + extra)
            .map(|i| {
                serde_json::json!({
                    "type": "response.function_call_arguments.done",
                    "call_id": format!("call_{i}"),
                    "name": "write_note",
                    "arguments": "{}"
                })
            })
            .collect();

        run_read_loop(
            frame_stream(frames),
            Arc::new(OpenAiRealtime::new()) as Arc<dyn RealtimeProvider>,
            write_tx,
            cost,
            Arc::new(OkRecorder) as Arc<dyn RecorderAdapter>,
            disp.clone() as Arc<dyn DispatcherSeam>,
            sd_rx,
            emit,
            Arc::new(TokioMutex::new(None)),
            TEST_GENERATION,
            test_counter(),
            |_| {}, // no-op audio_handler (no device in test)
            Arc::new(AtomicBool::new(true)), // mic always running in tests
            |_| {}, // no-op stop_audio (no device in test)
            None, // no write task to abort in unit tests
            |_month: u32, _used: u64, _budget: BudgetConfig| {},
            |_call_id: &str, _tool: &str| {},
        )
        .await;

        // Exactly the cap was spawned/dispatched; the extra frames were skipped,
        // not crashed. (Before koe-wj2 the count would equal the full frame
        // count, MAX_INFLIGHT_DISPATCHES + extra.)
        let dispatched = disp.calls.lock().unwrap().len();
        assert_eq!(
            dispatched, MAX_INFLIGHT_DISPATCHES,
            "in-flight tool dispatches must be capped at MAX_INFLIGHT_DISPATCHES, got {dispatched}"
        );
    }

    #[tokio::test]
    async fn usage_over_budget_stops_fail_closed() {
        // Budget = $0.000001 so a tiny audio usage trips it immediately.
        let cost = Arc::new(TokioMutex::new(CostTracker::new(
            BudgetConfig { enabled: true, monthly_limit_nanodollars: NANODOLLARS_PER_USD / 1_000_000 },
            current_yyyymm(),
        )));
        let (write_tx, _write_rx) = mpsc::channel::<Message>(8);
        let (_sd_tx, sd_rx) = oneshot::channel();
        let (log, emit) = collect_emit();
        let disp = Arc::new(RecordingDispatcher { calls: StdMutex::new(Vec::new()) });

        let frames = vec![
            serde_json::json!({
                "type": "response.done",
                "response": { "usage": { "input_token_details": { "audio_tokens": 1_000_000 } } }
            }),
            // This second frame must NOT be processed — the loop stops on the first.
            serde_json::json!({
                "type": "response.function_call_arguments.done",
                "call_id": "should_not_run", "name": "write_note", "arguments": "{}"
            }),
        ];
        run_read_loop(
            frame_stream(frames),
            Arc::new(OpenAiRealtime::new()) as Arc<dyn RealtimeProvider>,
            write_tx,
            cost.clone(),
            Arc::new(OkRecorder) as Arc<dyn RecorderAdapter>,
            disp.clone() as Arc<dyn DispatcherSeam>,
            sd_rx,
            emit,
            Arc::new(TokioMutex::new(None)),
            TEST_GENERATION,
            test_counter(),
            |_| {}, // no-op audio_handler (no device in test)
            Arc::new(AtomicBool::new(true)), // mic always running in tests
            |_| {}, // no-op stop_audio (no device in test)
            None, // no write task to abort in unit tests
            |_month: u32, _used: u64, _budget: BudgetConfig| {},
            |_call_id: &str, _tool: &str| {},
        )
        .await;

        let events = log.lock().unwrap();
        // budget-exceeded error, then idle on loop exit.
        assert!(events.iter().any(|(s, e)| s == "error" && e.as_deref() == Some("monthly budget exceeded")));
        assert!(cost.try_lock().unwrap().is_over_budget());
        // The function_call frame AFTER the budget trip must NOT be dispatched.
        assert!(disp.calls.lock().unwrap().is_empty(), "no dispatch after budget stop");
    }

    // ---- koe-ixt: cost-snapshot stop->start handover (fail-open guard) --------

    /// Recorder double modeling the SHARED cost ledger exactly as the real
    /// `SqliteAdapter` does after koe-ixt: an ADDITIVE accumulation that returns the
    /// new running total. The `persisted` map is `Arc`-shared so two simulated
    /// session loops (an older one draining late usage, a newer one just started)
    /// observe each other's adds through ONE ledger — the cross-session coupling the
    /// fix relies on (in production a single `ManagedRecorder` Arc is shared across
    /// sessions). Only the cost-ledger methods are real.
    struct SharedSnapshotRecorder {
        persisted: Arc<StdMutex<std::collections::HashMap<u32, u64>>>,
    }
    impl SharedSnapshotRecorder {
        fn new() -> (Self, Arc<StdMutex<std::collections::HashMap<u32, u64>>>) {
            let persisted = Arc::new(StdMutex::new(std::collections::HashMap::new()));
            (
                Self {
                    persisted: Arc::clone(&persisted),
                },
                persisted,
            )
        }
    }
    impl RecorderAdapter for SharedSnapshotRecorder {
        fn save_note(&self, _t: &str) -> Result<i64, RecorderError> {
            unimplemented!()
        }
        fn list_recent_notes(&self, _l: u32) -> Result<Vec<Note>, RecorderError> {
            unimplemented!()
        }
        fn log_conversation_event(&self, _r: &str, _k: &str, _s: &str) -> Result<i64, RecorderError> {
            unimplemented!()
        }
        fn list_recent_events(&self, _l: u32) -> Result<Vec<ConversationEvent>, RecorderError> {
            unimplemented!()
        }
        fn add_month_cost(&self, m: u32, n: u64) -> Result<u64, RecorderError> {
            // Additive accumulation (mirrors the real adapter): each add sums onto
            // the month's running total (saturating) and returns the new total, so
            // two sessions' spend through the SAME shared store is summed.
            let mut map = self.persisted.lock().unwrap();
            let new_total = map.get(&m).copied().unwrap_or(0).saturating_add(n);
            map.insert(m, new_total);
            Ok(new_total)
        }
        fn load_cost_snapshot(&self, m: u32) -> Result<Option<u64>, RecorderError> {
            Ok(self.persisted.lock().unwrap().get(&m).copied())
        }
        fn health_check(&self) -> Result<(), RecorderError> {
            Ok(())
        }
    }

    /// Drives one `response.done` usage frame through `handle_text` with the given
    /// tracker + recorder and returns the resulting `LoopAction`. Centralizes the
    /// 11-arg call so the handover test reads as a sequence of usage events.
    async fn drive_usage(
        usage: &Value,
        provider: &Arc<dyn RealtimeProvider>,
        recorder: &Arc<dyn RecorderAdapter>,
        cost: &Arc<TokioMutex<CostTracker>>,
    ) -> LoopAction {
        let (write_tx, _write_rx) = mpsc::channel::<Message>(8);
        let (rec_tx, _rec_rx) = mpsc::channel::<ConversationRecord>(8);
        let dispatcher: Arc<dyn DispatcherSeam> = Arc::new(NoopDispatcher);
        let mut dispatch_tasks = tokio::task::JoinSet::new();
        let mut save_failures = 0u32;
        let mut pending = PendingCost::default();
        let (mut cap_warned, mut journal_drop_warned) = (false, false);
        handle_text(
            usage,
            provider,
            &write_tx,
            cost,
            recorder,
            &rec_tx,
            &dispatcher,
            &mut dispatch_tasks,
            &mut save_failures,
            &mut pending,
            &mut cap_warned,
            &mut journal_drop_warned,
        &|_mo: u32, _us: u64, _bg: BudgetConfig| {},
        &|_ci: &str, _tn: &str| {},)
        .await
    }

    #[tokio::test]
    async fn handover_late_usage_stops_newer_session_via_global_total() {
        // koe-ixt mechanism 4 (fail-open). The stop->start cost handover, modeled
        // with an explicit, deterministic ledger ORDERING (no timing):
        //   0. The month already had $10 of spend in the shared ledger; both A and
        //      B loaded it as their baseline.
        //   1. A's read loop is still draining one late `response.done` (+$30): A
        //      adds it to the ledger ($10 -> $40) and stops itself fail-closed
        //      ($40 >= the $32 cap).
        //   2. B then drains a SMALL usage (+$1). B's LOCAL tracker is only $11
        //      (< $32), but the additive ledger now reads $41.
        // B must stop fail-closed by gating on the authoritative cross-session
        // ledger total, NOT its stale local baseline. Before the fix (gate on local)
        // B keeps charging fail-open — exactly the bug this issue closes.
        let month = current_yyyymm();
        let limit = 32 * NANODOLLARS_PER_USD; // $32 monthly cap
        let baseline = 10 * NANODOLLARS_PER_USD; // the month's prior spend

        // ONE shared ledger, as in production (a single ManagedRecorder Arc), seeded
        // with the $10 both sessions loaded as their baseline.
        let (rec, persisted) = SharedSnapshotRecorder::new();
        let recorder: Arc<dyn RecorderAdapter> = Arc::new(rec);
        recorder.add_month_cost(month, baseline).unwrap();
        let provider: Arc<dyn RealtimeProvider> = Arc::new(OpenAiRealtime::new());
        let budget = BudgetConfig {
            enabled: true,
            monthly_limit_nanodollars: limit,
        };

        // --- (1) Session A's late usage: +$30 audio input (937_500 * 32_000 nano). ---
        let cost_a = Arc::new(TokioMutex::new({
            let mut t = CostTracker::new(budget, month);
            t.month_total_nanodollars = baseline;
            t
        }));
        let a_late = serde_json::json!({
            "type": "response.done",
            "response": { "usage": { "input_token_details": { "audio_tokens": 937_500u64 } } }
        });
        let a_result = drive_usage(&a_late, &provider, &recorder, &cost_a).await;
        // A is over its OWN local cap ($40 >= $32) too, so it stops fail-closed.
        assert!(
            matches!(a_result, LoopAction::Stop("monthly budget exceeded")),
            "A's own loop must stop once its late usage exceeds the cap"
        );
        // A's add brought the SHARED ledger to $40 (seed $10 + $30).
        assert_eq!(
            *persisted.lock().unwrap().get(&month).unwrap(),
            40 * NANODOLLARS_PER_USD,
            "A's late add must bring the shared ledger to $40"
        );

        // --- (2) Session B's small usage: +$1 audio input (31_250 * 32_000 nano). ---
        let cost_b = Arc::new(TokioMutex::new({
            let mut t = CostTracker::new(budget, month);
            t.month_total_nanodollars = baseline; // B loaded the SAME stale $10 baseline
            t
        }));
        let b_small = serde_json::json!({
            "type": "response.done",
            "response": { "usage": { "input_token_details": { "audio_tokens": 31_250u64 } } }
        });
        let b_result = drive_usage(&b_small, &provider, &recorder, &cost_b).await;

        // THE FIX: B fails closed on the authoritative ledger total ($41 >= $32) even
        // though its own local total is only $11. Pre-fix (local-only gate) this was
        // LoopAction::Continue — the newer session billing fail-open.
        assert!(
            matches!(b_result, LoopAction::Stop("monthly budget exceeded")),
            "newer session B must fail-closed on the cross-session ledger, not its stale local baseline (koe-ixt mechanism 4)"
        );
        // The ledger SUMMED both sessions' spend ($10 seed + A's $30 + B's $1 = $41),
        // not max'd them ($40) — so a handover sibling's spend is never lost.
        assert_eq!(
            *persisted.lock().unwrap().get(&month).unwrap(),
            41 * NANODOLLARS_PER_USD,
            "the ledger must SUM both sessions' spend ($41), not keep only the max ($40)"
        );
        // B's own local total alone ($11) is under the cap, proving the stop came
        // from the shared ledger total and not B's local baseline.
        assert!(
            !cost_b.lock().await.is_over_budget(),
            "B's local total ($11) must be under the cap — the stop must come from the shared ledger"
        );
    }

    #[tokio::test]
    async fn save_failure_with_over_cap_local_stops_fail_closed() {
        // koe-ixt (R-B finding): a NEW fail-closed branch. When the ledger ADD
        // fails transiently (below the MAX_SNAPSHOT_SAVE_FAILURES terminal
        // threshold) but THIS frame's spend already exceeds the cap, the loop must
        // STILL stop on "monthly budget exceeded" via the fallback gate — NOT keep
        // charging until the failure counter trips. FailingRecorder fails the add
        // and returns Ok(None) for the readback, so the fallback gate sees
        // 0 + this frame's delta, which is over the (tiny) cap → fail-closed on the
        // first frame (distinct from the "cost tracking unavailable" terminal path,
        // which needs MAX_SNAPSHOT_SAVE_FAILURES consecutive failures).
        let month = current_yyyymm();
        let cost = Arc::new(TokioMutex::new(CostTracker::new(
            BudgetConfig {
                enabled: true,
                monthly_limit_nanodollars: NANODOLLARS_PER_USD / 1_000_000, // tiny cap
            },
            month,
        )));
        let provider: Arc<dyn RealtimeProvider> = Arc::new(OpenAiRealtime::new());
        let recorder: Arc<dyn RecorderAdapter> = Arc::new(FailingRecorder);
        let usage = serde_json::json!({
            "type": "response.done",
            "response": { "usage": { "input_token_details": { "audio_tokens": 1_000_000u64 } } }
        });
        let result = drive_usage(&usage, &provider, &recorder, &cost).await;
        assert!(
            matches!(result, LoopAction::Stop("monthly budget exceeded")),
            "an over-cap LOCAL spend must stop fail-closed on the first transient save failure, not charge on"
        );
    }

    #[tokio::test]
    async fn usage_is_persisted_under_the_tracker_effective_month_not_the_clock() {
        // koe-ixt backward-clock guard: the snapshot is keyed on the tracker's
        // EFFECTIVE accounting month (c.current_month), NOT the raw observed clock
        // month. `add_usage` only advances current_month FORWARD, so a tracker
        // already at a LATER month than the clock (modeling a backward clock skew /
        // NTP step across a month boundary) keeps its month, and the usage must
        // persist under THAT month — otherwise the save/gate would target the wrong
        // month row and miss an already-over-cap current month (fail-open).
        let future_month = 209912; // strictly ahead of any real current_yyyymm()
        assert!(
            future_month > current_yyyymm(),
            "test premise: the tracker's month must be ahead of the wall clock"
        );
        let cost = Arc::new(TokioMutex::new(CostTracker::new(
            BudgetConfig {
                enabled: false,
                monthly_limit_nanodollars: 0,
            },
            future_month,
        )));
        let (rec, persisted) = SharedSnapshotRecorder::new();
        let recorder: Arc<dyn RecorderAdapter> = Arc::new(rec);
        let provider: Arc<dyn RealtimeProvider> = Arc::new(OpenAiRealtime::new());
        let usage = serde_json::json!({
            "type": "response.done",
            "response": { "usage": { "input_token_details": { "audio_tokens": 100u64 } } }
        });
        let _ = drive_usage(&usage, &provider, &recorder, &cost).await;

        let map = persisted.lock().unwrap();
        assert!(
            map.contains_key(&future_month),
            "usage must persist under the tracker's effective month ({future_month}), got keys {:?}",
            map.keys().collect::<Vec<_>>()
        );
        assert!(
            !map.contains_key(&current_yyyymm()),
            "usage must NOT persist under the raw observed clock month"
        );
    }

    /// Recorder whose cost-snapshot SAVE *and* LOAD both fail — models a DB that is
    /// fully unavailable, so the authoritative balance is unknowable.
    struct ReadWriteFailRecorder;
    impl RecorderAdapter for ReadWriteFailRecorder {
        fn save_note(&self, _t: &str) -> Result<i64, RecorderError> {
            unimplemented!()
        }
        fn list_recent_notes(&self, _l: u32) -> Result<Vec<Note>, RecorderError> {
            unimplemented!()
        }
        fn log_conversation_event(&self, _r: &str, _k: &str, _s: &str) -> Result<i64, RecorderError> {
            unimplemented!()
        }
        fn list_recent_events(&self, _l: u32) -> Result<Vec<ConversationEvent>, RecorderError> {
            unimplemented!()
        }
        fn add_month_cost(&self, _m: u32, _n: u64) -> Result<u64, RecorderError> {
            Err(RecorderError::Db)
        }
        fn load_cost_snapshot(&self, _m: u32) -> Result<Option<u64>, RecorderError> {
            Err(RecorderError::Db)
        }
        fn health_check(&self) -> Result<(), RecorderError> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn readback_failure_after_save_failure_stops_fail_closed_immediately() {
        // koe-ixt (CodeRabbit Major): when BOTH the snapshot SAVE and the recovery
        // READ fail, the authoritative balance is UNKNOWN. Fail-closed — stop with
        // "cost tracking unavailable" IMMEDIATELY (on the first failure), NOT after
        // MAX_SNAPSHOT_SAVE_FAILURES: an unknown balance must never permit continued
        // charging. (Contrast Ok(None) = a known-empty month, which falls through to
        // the local-total gate + the failure counter — see
        // repeated_snapshot_save_failure_stops_fail_closed / FailingRecorder.)
        let month = current_yyyymm();
        let cost = Arc::new(TokioMutex::new(CostTracker::new(
            BudgetConfig {
                enabled: true,
                monthly_limit_nanodollars: 32 * NANODOLLARS_PER_USD,
            },
            month,
        )));
        let provider: Arc<dyn RealtimeProvider> = Arc::new(OpenAiRealtime::new());
        let recorder: Arc<dyn RecorderAdapter> = Arc::new(ReadWriteFailRecorder);
        // A tiny, well-under-cap usage: the stop must come from the UNKNOWN balance,
        // not from the budget gate.
        let usage = serde_json::json!({
            "type": "response.done",
            "response": { "usage": { "input_token_details": { "audio_tokens": 1u64 } } }
        });
        let result = drive_usage(&usage, &provider, &recorder, &cost).await;
        assert!(
            matches!(result, LoopAction::Stop("cost tracking unavailable")),
            "an unknown authoritative balance (save AND read both failed) must stop fail-closed on the first frame"
        );
    }

    #[tokio::test]
    async fn failed_ledger_adds_carry_forward_and_still_trip_the_cap() {
        // koe-ixt (Codex R-C): a failed `add_month_cost` must NOT silently drop this
        // frame's spend from the ledger. The unpersisted delta is carried forward
        // (`pending`) so the gate keeps counting it across frames. Codex's
        // scenario: a $15 cap and TWO consecutive add-failures of +$10 each. True
        // spend is $20, so the second frame must fail-closed on the CARRIED $10 + the
        // new $10 = $20 (>= $15) — even though each frame alone is only $10 and
        // neither was persisted. Without the carry the gate would see only $10 each
        // time and charge on (fail-open / undercount).
        let month = current_yyyymm();
        let cost = Arc::new(TokioMutex::new(CostTracker::new(
            BudgetConfig {
                enabled: true,
                monthly_limit_nanodollars: 15 * NANODOLLARS_PER_USD,
            },
            month,
        )));
        let provider: Arc<dyn RealtimeProvider> = Arc::new(OpenAiRealtime::new());
        // FailingRecorder: add_month_cost -> Err, load_cost_snapshot -> Ok(None), so
        // the fallback gate sees 0 + the carried unpersisted amount.
        let recorder: Arc<dyn RecorderAdapter> = Arc::new(FailingRecorder);
        // $10 per frame = 312_500 audio-input tokens * 32_000 nanodollars.
        let ten_dollars = serde_json::json!({
            "type": "response.done",
            "response": { "usage": { "input_token_details": { "audio_tokens": 312_500u64 } } }
        });
        let (write_tx, _write_rx) = mpsc::channel::<Message>(8);
        let (rec_tx, _rec_rx) = mpsc::channel::<ConversationRecord>(8);
        let dispatcher: Arc<dyn DispatcherSeam> = Arc::new(NoopDispatcher);
        let mut dispatch_tasks = tokio::task::JoinSet::new();
        // Persistent state across both frames (run_read_loop owns these for the
        // whole session, so the carry survives between frames).
        let mut save_failures = 0u32;
        let mut pending = PendingCost::default();
        let (mut cap_warned, mut journal_drop_warned) = (false, false);

        // Frame 1: add fails; the $10 is carried, gate sees only $10 (< $15 cap).
        let r1 = handle_text(
            &ten_dollars,
            &provider,
            &write_tx,
            &cost,
            &recorder,
            &rec_tx,
            &dispatcher,
            &mut dispatch_tasks,
            &mut save_failures,
            &mut pending,
            &mut cap_warned,
            &mut journal_drop_warned,
        &|_mo: u32, _us: u64, _bg: BudgetConfig| {},
        &|_ci: &str, _tn: &str| {},)
        .await;
        assert!(
            matches!(r1, LoopAction::Continue),
            "the first $10 (< $15 cap) must continue"
        );
        assert_eq!(
            pending.nanodollars,
            10 * NANODOLLARS_PER_USD,
            "the failed $10 add must be carried forward, not dropped from the ledger"
        );

        // Frame 2: add fails again; gate sees carried $10 + new $10 = $20 (>= $15) →
        // fail-closed. This is the budget gate, NOT the 'cost tracking unavailable'
        // counter path (only 2 failures so far, below MAX_SNAPSHOT_SAVE_FAILURES).
        let r2 = handle_text(
            &ten_dollars,
            &provider,
            &write_tx,
            &cost,
            &recorder,
            &rec_tx,
            &dispatcher,
            &mut dispatch_tasks,
            &mut save_failures,
            &mut pending,
            &mut cap_warned,
            &mut journal_drop_warned,
        &|_mo: u32, _us: u64, _bg: BudgetConfig| {},
        &|_ci: &str, _tn: &str| {},)
        .await;
        assert!(
            matches!(r2, LoopAction::Stop("monthly budget exceeded")),
            "the carried + new delta ($20 >= $15) must trip the cap fail-closed, not be lost"
        );
    }

    #[tokio::test]
    async fn pending_cost_from_a_past_month_is_dropped_not_folded_into_new_month() {
        // koe-ixt (Codex P2): the carried unpersisted spend is MONTH-SCOPED. If a
        // month rollover happens while spend is still unpersisted (a prior add
        // failed), the stale carry must NOT be folded into the new month's row (that
        // would over-count the new month). It is DROPPED — the old month's cap was
        // already enforced live and its persisted total is never read again once it
        // is no longer the current month — and the loop PROCEEDS so this frame's
        // spend is still recorded in (and gated against) the NEW month, not lost to
        // an early stop. Modeled by pre-seeding `pending` for an EARLIER month than
        // the tracker's effective month and driving one frame.
        let tracker_month = 209912; // strictly ahead of any real current_yyyymm()
        assert!(
            tracker_month > current_yyyymm(),
            "test premise: the tracker's month must be ahead of the wall clock"
        );
        let (rec, persisted) = SharedSnapshotRecorder::new();
        let recorder: Arc<dyn RecorderAdapter> = Arc::new(rec);
        let cost = Arc::new(TokioMutex::new(CostTracker::new(
            BudgetConfig {
                enabled: true,
                monthly_limit_nanodollars: 1000 * NANODOLLARS_PER_USD,
            },
            tracker_month,
        )));
        let provider: Arc<dyn RealtimeProvider> = Arc::new(OpenAiRealtime::new());
        // This frame's spend: $10 = 312_500 audio-input tokens * 32_000 nanodollars.
        let ten_dollars = serde_json::json!({
            "type": "response.done",
            "response": { "usage": { "input_token_details": { "audio_tokens": 312_500u64 } } }
        });
        let (write_tx, _write_rx) = mpsc::channel::<Message>(8);
        let (rec_tx, _rec_rx) = mpsc::channel::<ConversationRecord>(8);
        let dispatcher: Arc<dyn DispatcherSeam> = Arc::new(NoopDispatcher);
        let mut dispatch_tasks = tokio::task::JoinSet::new();
        let mut save_failures = 0u32;
        // Unpersisted $5 left over from an EARLIER month (tracker_month - 1).
        let mut pending = PendingCost {
            month: tracker_month - 1,
            nanodollars: 5 * NANODOLLARS_PER_USD,
        };
        let (mut cap_warned, mut journal_drop_warned) = (false, false);

        let result = handle_text(
            &ten_dollars,
            &provider,
            &write_tx,
            &cost,
            &recorder,
            &rec_tx,
            &dispatcher,
            &mut dispatch_tasks,
            &mut save_failures,
            &mut pending,
            &mut cap_warned,
            &mut journal_drop_warned,
        &|_mo: u32, _us: u64, _bg: BudgetConfig| {},
        &|_ci: &str, _tn: &str| {},)
        .await;
        assert!(
            matches!(result, LoopAction::Continue),
            "the session must proceed (recording this frame), not stop, on a rollover with stale pending"
        );
        // The NEW month's ledger got ONLY this frame's $10 — the stale $5 from the
        // old month was DROPPED, not folded into the new month's total.
        assert_eq!(
            *persisted.lock().unwrap().get(&tracker_month).unwrap(),
            10 * NANODOLLARS_PER_USD,
            "stale old-month pending ($5) must be dropped, not added to the new month ($10, not $15)"
        );
        // The dropped old-month pending was not written to the old month's row either.
        assert!(
            !persisted.lock().unwrap().contains_key(&(tracker_month - 1)),
            "the dropped old-month pending must not be written anywhere"
        );
        // After a successful add the carry is reset and re-scoped to the new month.
        assert_eq!(pending.nanodollars, 0, "carry is cleared after a successful add");
        assert_eq!(pending.month, tracker_month, "carry is re-scoped to the current month");
    }

    #[tokio::test]
    async fn handover_below_cap_sibling_spend_is_summed_not_lost() {
        // koe-ixt (Codex P1, the max-vs-sum fail-open). The exact scenario an
        // absolute-total + max() scheme fails: a $45 cap, a handover sibling (A) that
        // added $40 (below the cap), and a newer session B (loaded $0) that drains a
        // $10 frame. The TRUE month total is $50 (> $45). An additive ledger sums
        // them ($40 + $10 = $50) and B stops fail-closed; a max() scheme would keep
        // only max($40, $10) = $40 (< $45) and let B charge on — fail-open.
        let month = current_yyyymm();
        let budget = BudgetConfig {
            enabled: true,
            monthly_limit_nanodollars: 45 * NANODOLLARS_PER_USD,
        };
        let (rec, persisted) = SharedSnapshotRecorder::new();
        let recorder: Arc<dyn RecorderAdapter> = Arc::new(rec);
        let provider: Arc<dyn RealtimeProvider> = Arc::new(OpenAiRealtime::new());
        // Sibling A's drained late spend ($40), already in the shared ledger; below
        // the $45 cap on its own.
        recorder.add_month_cost(month, 40 * NANODOLLARS_PER_USD).unwrap();
        // B loaded $0 (it started before A's add landed) and drains a $10 frame
        // (312_500 audio-input tokens * 32_000 nanodollars).
        let cost_b = Arc::new(TokioMutex::new(CostTracker::new(budget, month)));
        let ten_dollars = serde_json::json!({
            "type": "response.done",
            "response": { "usage": { "input_token_details": { "audio_tokens": 312_500u64 } } }
        });
        let result = drive_usage(&ten_dollars, &provider, &recorder, &cost_b).await;

        assert!(
            matches!(result, LoopAction::Stop("monthly budget exceeded")),
            "B must fail-closed once the SUMMED ledger ($40 + $10 = $50) exceeds the $45 cap (a max() scheme would fail open at $40)"
        );
        assert_eq!(
            *persisted.lock().unwrap().get(&month).unwrap(),
            50 * NANODOLLARS_PER_USD,
            "the ledger must SUM the sibling's $40 and B's $10 to $50, not keep only the $40 max"
        );
        // B's own local spend alone ($10) is far under the cap, proving the stop came
        // from the summed cross-session ledger.
        assert!(
            !cost_b.lock().await.is_over_budget(),
            "B's local total ($10) is under the cap — the stop must come from the summed ledger"
        );
    }

    #[tokio::test]
    async fn shutdown_signal_breaks_loop() {
        let cost = Arc::new(TokioMutex::new(CostTracker::new(
            BudgetConfig { enabled: false, monthly_limit_nanodollars: 0 },
            current_yyyymm(),
        )));
        let (write_tx, _write_rx) = mpsc::channel::<Message>(8);
        let (sd_tx, sd_rx) = oneshot::channel();
        let (log, emit) = collect_emit();
        // Fire shutdown before the loop runs; it must exit promptly and emit idle.
        sd_tx.send(()).unwrap();
        run_read_loop(
            frame_stream(vec![]),
            Arc::new(OpenAiRealtime::new()) as Arc<dyn RealtimeProvider>,
            write_tx,
            cost,
            Arc::new(OkRecorder) as Arc<dyn RecorderAdapter>,
            Arc::new(NoopDispatcher) as Arc<dyn DispatcherSeam>,
            sd_rx,
            emit,
            Arc::new(TokioMutex::new(None)),
            TEST_GENERATION,
            test_counter(),
            |_| {}, // no-op audio_handler (no device in test)
            Arc::new(AtomicBool::new(true)), // mic always running in tests
            |_| {}, // no-op stop_audio (no device in test)
            None, // no write task to abort in unit tests
            |_month: u32, _used: u64, _budget: BudgetConfig| {},
            |_call_id: &str, _tool: &str| {},
        )
        .await;
        assert!(log.lock().unwrap().iter().any(|(s, _)| s == "idle"));
    }

    /// Recorder whose cost-snapshot saves always fail (for the save-failure path).
    struct FailingRecorder;
    impl RecorderAdapter for FailingRecorder {
        fn save_note(&self, _t: &str) -> Result<i64, RecorderError> {
            unimplemented!()
        }
        fn list_recent_notes(&self, _l: u32) -> Result<Vec<Note>, RecorderError> {
            unimplemented!()
        }
        fn log_conversation_event(&self, _r: &str, _k: &str, _s: &str) -> Result<i64, RecorderError> {
            // Fails like every write here; not exercised by the cost-snapshot
            // test below, but a real (non-panicking) error keeps this double safe
            // if a future test routes a transcript / tool frame through it.
            Err(RecorderError::Db)
        }
        fn list_recent_events(&self, _l: u32) -> Result<Vec<ConversationEvent>, RecorderError> {
            unimplemented!()
        }
        fn add_month_cost(&self, _m: u32, _n: u64) -> Result<u64, RecorderError> {
            Err(RecorderError::Db)
        }
        fn load_cost_snapshot(&self, _m: u32) -> Result<Option<u64>, RecorderError> {
            Ok(None)
        }
        fn health_check(&self) -> Result<(), RecorderError> {
            Ok(())
        }
    }

    /// Recorder whose conversation-log writes always fail but whose cost snapshot
    /// succeeds — isolates the koe-emd fail-soft path (a failed log must NOT stop
    /// the session) from the cost-tracking fail-closed path. Counts log attempts
    /// so a test can prove the failing write was actually attempted-and-swallowed
    /// (not merely never sent).
    struct LogFailRecorder {
        log_attempts: Arc<AtomicUsize>,
    }
    impl LogFailRecorder {
        fn new() -> (Self, Arc<AtomicUsize>) {
            let n = Arc::new(AtomicUsize::new(0));
            (
                Self {
                    log_attempts: Arc::clone(&n),
                },
                n,
            )
        }
    }
    impl RecorderAdapter for LogFailRecorder {
        fn save_note(&self, _t: &str) -> Result<i64, RecorderError> {
            unimplemented!()
        }
        fn list_recent_notes(&self, _l: u32) -> Result<Vec<Note>, RecorderError> {
            unimplemented!()
        }
        fn log_conversation_event(&self, _r: &str, _k: &str, _s: &str) -> Result<i64, RecorderError> {
            self.log_attempts.fetch_add(1, Ordering::Relaxed);
            Err(RecorderError::Db)
        }
        fn list_recent_events(&self, _l: u32) -> Result<Vec<ConversationEvent>, RecorderError> {
            unimplemented!()
        }
        fn add_month_cost(&self, _m: u32, n: u64) -> Result<u64, RecorderError> {
            Ok(n)
        }
        fn load_cost_snapshot(&self, _m: u32) -> Result<Option<u64>, RecorderError> {
            Ok(None)
        }
        fn health_check(&self) -> Result<(), RecorderError> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn failed_conversation_log_does_not_stop_session() {
        // A recorder error on a transcript / tool turn must be swallowed
        // (koe-emd fail-soft): the loop keeps processing later frames (the tool
        // still dispatches) and exits cleanly (idle), never an error status.
        let disp = Arc::new(RecordingDispatcher { calls: StdMutex::new(Vec::new()) });
        let (rec, log_attempts) = LogFailRecorder::new();
        let cost = Arc::new(TokioMutex::new(CostTracker::new(
            BudgetConfig { enabled: false, monthly_limit_nanodollars: 0 },
            current_yyyymm(),
        )));
        let (write_tx, _write_rx) = mpsc::channel::<Message>(8);
        let (_sd_tx, sd_rx) = oneshot::channel();
        let (log, emit) = collect_emit();

        let frames = vec![
            serde_json::json!({
                "type": "conversation.item.input_audio_transcription.completed",
                "transcript": "hello there"
            }),
            serde_json::json!({
                "type": "response.function_call_arguments.done",
                "call_id": "call_1",
                "name": "web_search",
                "arguments": "{}"
            }),
        ];
        run_read_loop(
            frame_stream(frames),
            Arc::new(OpenAiRealtime::new()) as Arc<dyn RealtimeProvider>,
            write_tx,
            cost,
            Arc::new(rec) as Arc<dyn RecorderAdapter>,
            disp.clone() as Arc<dyn DispatcherSeam>,
            sd_rx,
            emit,
            Arc::new(TokioMutex::new(None)),
            TEST_GENERATION,
            test_counter(),
            |_| {},
            Arc::new(AtomicBool::new(true)),
            |_| {},
            None,
        |_month: u32, _used: u64, _budget: BudgetConfig| {},
        |_call_id: &str, _tool: &str| {},)
        .await;

        // Both journal writes (the transcript + the tool turn) were actually
        // attempted and the errors swallowed — not silently skipped. run_read_loop
        // awaits the writer before returning, so the count is settled here.
        assert_eq!(
            log_attempts.load(Ordering::Relaxed),
            2,
            "both the transcript and tool turns must reach (and fail at) the recorder"
        );
        // The loop continued past the failed transcript record and dispatched.
        assert_eq!(disp.calls.lock().unwrap().as_slice(), ["web_search"]);
        // It exited cleanly: no error status despite both log writes failing.
        let emits = log.lock().unwrap();
        assert!(
            !emits.iter().any(|(s, _)| s == "error"),
            "a failed log must not emit error: {emits:?}"
        );
        assert!(
            emits.iter().any(|(s, _)| s == "idle"),
            "expected idle on normal close: {emits:?}"
        );
    }

    #[tokio::test]
    async fn usage_frame_is_not_journalled() {
        // Invariant 2 (no double recording): usage is persisted via the cost
        // snapshot, NOT the conversation log. A response.done usage frame must
        // produce ZERO conversation rows — guards against a future contributor
        // "also logging usage" in the Usage arm, which would double-record turns.
        let (rec, events) = RecordingRecorder::new();
        let cost = Arc::new(TokioMutex::new(CostTracker::new(
            BudgetConfig { enabled: false, monthly_limit_nanodollars: 0 },
            current_yyyymm(),
        )));
        let (write_tx, _write_rx) = mpsc::channel::<Message>(8);
        let (_sd_tx, sd_rx) = oneshot::channel();
        let (_log, emit) = collect_emit();

        let frames = vec![serde_json::json!({
            "type": "response.done",
            "response": { "usage": {
                "input_token_details": { "audio_tokens": 10, "text_tokens": 1, "cached_tokens": 0 },
                "output_token_details": { "audio_tokens": 20, "text_tokens": 2 }
            }}
        })];
        run_read_loop(
            frame_stream(frames),
            Arc::new(OpenAiRealtime::new()) as Arc<dyn RealtimeProvider>,
            write_tx,
            cost,
            Arc::new(rec) as Arc<dyn RecorderAdapter>,
            Arc::new(NoopDispatcher) as Arc<dyn DispatcherSeam>,
            sd_rx,
            emit,
            Arc::new(TokioMutex::new(None)),
            TEST_GENERATION,
            test_counter(),
            |_| {},
            Arc::new(AtomicBool::new(true)),
            |_| {},
            None,
        |_month: u32, _used: u64, _budget: BudgetConfig| {},
        |_call_id: &str, _tool: &str| {},)
        .await;

        assert!(
            events.lock().unwrap().is_empty(),
            "a usage frame must not produce a conversation-log row (cost snapshot owns usage)"
        );
    }

    #[tokio::test]
    async fn recorded_turn_survives_abnormal_exit() {
        // The tail-drain on loop exit is unconditional: a turn that already
        // happened must be persisted even when the session ends abnormally (here a
        // connection error). Guards the "turns belong in the history even on an
        // abnormal exit" contract — a regression that moved the drain into the
        // normal-close branch would lose the tail on every error exit.
        let (rec, events) = RecordingRecorder::new();
        let cost = Arc::new(TokioMutex::new(CostTracker::new(
            BudgetConfig { enabled: false, monthly_limit_nanodollars: 0 },
            current_yyyymm(),
        )));
        let (write_tx, _write_rx) = mpsc::channel::<Message>(8);
        let (_sd_tx, sd_rx) = oneshot::channel();
        let (log, emit) = collect_emit();
        // A user transcript, then an abnormal connection-error exit.
        let stream = futures_util::stream::iter(vec![
            Ok(Message::Text(
                serde_json::json!({
                    "type": "conversation.item.input_audio_transcription.completed",
                    "transcript": "remember this"
                })
                .to_string()
                .into(),
            )),
            Err(WsError::ConnectionClosed),
        ]);
        run_read_loop(
            stream,
            Arc::new(OpenAiRealtime::new()) as Arc<dyn RealtimeProvider>,
            write_tx,
            cost,
            Arc::new(rec) as Arc<dyn RecorderAdapter>,
            Arc::new(NoopDispatcher) as Arc<dyn DispatcherSeam>,
            sd_rx,
            emit,
            Arc::new(TokioMutex::new(None)),
            TEST_GENERATION,
            test_counter(),
            |_| {},
            Arc::new(AtomicBool::new(true)),
            |_| {},
            None,
        |_month: u32, _used: u64, _budget: BudgetConfig| {},
        |_call_id: &str, _tool: &str| {},)
        .await;

        // The abnormal exit emitted the terminal error...
        assert!(
            log.lock().unwrap().iter().any(|(s, _)| s == "error"),
            "expected a terminal error status on abnormal exit"
        );
        // ...yet the turn that already happened was still flushed to the journal.
        assert_eq!(
            events.lock().unwrap().as_slice(),
            [("user".to_string(), "speech".to_string(), "remember this".to_string())]
        );
    }

    #[tokio::test]
    async fn connection_error_emits_and_exits() {
        let cost = Arc::new(TokioMutex::new(CostTracker::new(
            BudgetConfig { enabled: false, monthly_limit_nanodollars: 0 },
            current_yyyymm(),
        )));
        let (write_tx, _rx) = mpsc::channel::<Message>(8);
        let (_sd, sd_rx) = oneshot::channel();
        let (log, emit) = collect_emit();
        let stream = futures_util::stream::iter(vec![Err(WsError::ConnectionClosed)]);
        run_read_loop(
            stream,
            Arc::new(OpenAiRealtime::new()) as Arc<dyn RealtimeProvider>,
            write_tx,
            cost,
            Arc::new(OkRecorder) as Arc<dyn RecorderAdapter>,
            Arc::new(NoopDispatcher) as Arc<dyn DispatcherSeam>,
            sd_rx,
            emit,
            Arc::new(TokioMutex::new(None)),
            TEST_GENERATION,
            test_counter(),
            |_| {}, // no-op audio_handler (no device in test)
            Arc::new(AtomicBool::new(true)), // mic always running in tests
            |_| {}, // no-op stop_audio (no device in test)
            None, // no write task to abort in unit tests
            |_month: u32, _used: u64, _budget: BudgetConfig| {},
            |_call_id: &str, _tool: &str| {},
        )
        .await;
        let events = log.lock().unwrap();
        assert!(events.iter().any(|(s, e)| s == "error" && e.as_deref() == Some("connection error")));
        // error is terminal — no trailing idle that would clear the reason in the UI.
        assert!(!events.iter().any(|(s, _)| s == "idle"));
    }

    #[tokio::test]
    async fn server_close_drains_inflight_dispatch() {
        // A function call, then a server Close: the in-flight dispatch must be
        // DRAINED to completion (not aborted), so its frames are sent.
        let disp = Arc::new(RecordingDispatcher { calls: StdMutex::new(Vec::new()) });
        let cost = Arc::new(TokioMutex::new(CostTracker::new(
            BudgetConfig { enabled: false, monthly_limit_nanodollars: 0 },
            current_yyyymm(),
        )));
        let (write_tx, mut write_rx) = mpsc::channel::<Message>(8);
        let (_sd, sd_rx) = oneshot::channel();
        let (_log, emit) = collect_emit();
        let stream = futures_util::stream::iter(vec![
            Ok(Message::Text(
                serde_json::json!({
                    "type": "response.function_call_arguments.done",
                    "call_id": "c1", "name": "write_note", "arguments": "{}"
                })
                .to_string()
                .into(),
            )),
            Ok(Message::Close(None)),
        ]);
        run_read_loop(
            stream,
            Arc::new(OpenAiRealtime::new()) as Arc<dyn RealtimeProvider>,
            write_tx,
            cost,
            Arc::new(OkRecorder) as Arc<dyn RecorderAdapter>,
            disp.clone() as Arc<dyn DispatcherSeam>,
            sd_rx,
            emit,
            Arc::new(TokioMutex::new(None)),
            TEST_GENERATION,
            test_counter(),
            |_| {}, // no-op audio_handler (no device in test)
            Arc::new(AtomicBool::new(true)), // mic always running in tests
            |_| {}, // no-op stop_audio (no device in test)
            None, // no write task to abort in unit tests
            |_month: u32, _used: u64, _budget: BudgetConfig| {},
            |_call_id: &str, _tool: &str| {},
        )
        .await;
        assert_eq!(disp.calls.lock().unwrap().as_slice(), ["write_note"]);
        assert!(write_rx.recv().await.is_some(), "drained dispatch must have sent its frames");
    }

    #[tokio::test]
    async fn repeated_snapshot_save_failure_stops_fail_closed() {
        // Budget disabled so it never trips; the STOP must come from repeated
        // cost-snapshot save failures (durability fail-closed).
        let cost = Arc::new(TokioMutex::new(CostTracker::new(
            BudgetConfig { enabled: false, monthly_limit_nanodollars: 0 },
            current_yyyymm(),
        )));
        let (write_tx, _rx) = mpsc::channel::<Message>(8);
        let (_sd, sd_rx) = oneshot::channel();
        let (log, emit) = collect_emit();
        let usage = serde_json::json!({
            "type": "response.done",
            "response": { "usage": { "input_token_details": { "audio_tokens": 1 } } }
        });
        let frames = vec![usage.clone(), usage.clone(), usage.clone(), usage];
        run_read_loop(
            frame_stream(frames),
            Arc::new(OpenAiRealtime::new()) as Arc<dyn RealtimeProvider>,
            write_tx,
            cost,
            Arc::new(FailingRecorder) as Arc<dyn RecorderAdapter>,
            Arc::new(NoopDispatcher) as Arc<dyn DispatcherSeam>,
            sd_rx,
            emit,
            Arc::new(TokioMutex::new(None)),
            TEST_GENERATION,
            test_counter(),
            |_| {}, // no-op audio_handler (no device in test)
            Arc::new(AtomicBool::new(true)), // mic always running in tests
            |_| {}, // no-op stop_audio (no device in test)
            None, // no write task to abort in unit tests
            |_month: u32, _used: u64, _budget: BudgetConfig| {},
            |_call_id: &str, _tool: &str| {},
        )
        .await;
        assert!(log
            .lock()
            .unwrap()
            .iter()
            .any(|(s, e)| s == "error" && e.as_deref() == Some("cost tracking unavailable")));
    }

    #[tokio::test]
    async fn unparseable_frame_is_ignored_then_loop_continues() {
        let disp = Arc::new(RecordingDispatcher { calls: StdMutex::new(Vec::new()) });
        let cost = Arc::new(TokioMutex::new(CostTracker::new(
            BudgetConfig { enabled: false, monthly_limit_nanodollars: 0 },
            current_yyyymm(),
        )));
        let (write_tx, _rx) = mpsc::channel::<Message>(8);
        let (_sd, sd_rx) = oneshot::channel();
        let (_log, emit) = collect_emit();
        let stream = futures_util::stream::iter(vec![
            Ok(Message::Text("not json {{{".to_string().into())),
            Ok(Message::Text(
                serde_json::json!({
                    "type": "response.function_call_arguments.done",
                    "call_id": "c", "name": "write_note", "arguments": "{}"
                })
                .to_string()
                .into(),
            )),
        ]);
        run_read_loop(
            stream,
            Arc::new(OpenAiRealtime::new()) as Arc<dyn RealtimeProvider>,
            write_tx,
            cost,
            Arc::new(OkRecorder) as Arc<dyn RecorderAdapter>,
            disp.clone() as Arc<dyn DispatcherSeam>,
            sd_rx,
            emit,
            Arc::new(TokioMutex::new(None)),
            TEST_GENERATION,
            test_counter(),
            |_| {}, // no-op audio_handler (no device in test)
            Arc::new(AtomicBool::new(true)), // mic always running in tests
            |_| {}, // no-op stop_audio (no device in test)
            None, // no write task to abort in unit tests
            |_month: u32, _used: u64, _budget: BudgetConfig| {},
            |_call_id: &str, _tool: &str| {},
        )
        .await;
        // The unparseable frame was skipped and the following valid call dispatched.
        assert_eq!(disp.calls.lock().unwrap().as_slice(), ["write_note"]);
    }

    /// Verifies that `response.audio.delta` frames are forwarded to the
    /// `audio_handler` closure (the playback injection seam).  Checks both that
    /// the handler is called AND that non-audio frames are passed through without
    /// calling the handler.
    #[tokio::test]
    async fn audio_delta_frames_reach_audio_handler() {
        let cost = Arc::new(TokioMutex::new(CostTracker::new(
            BudgetConfig { enabled: false, monthly_limit_nanodollars: 0 },
            current_yyyymm(),
        )));
        let (write_tx, _rx) = mpsc::channel::<Message>(8);
        let (_sd, sd_rx) = oneshot::channel();
        let (_log, emit) = collect_emit();

        // Collect the event types seen by the audio_handler.
        let audio_calls: Arc<StdMutex<Vec<String>>> = Arc::new(StdMutex::new(Vec::new()));
        let audio_calls_clone = Arc::clone(&audio_calls);
        let audio_handler = move |event: &serde_json::Value| {
            if let Some(t) = event.get("type").and_then(serde_json::Value::as_str) {
                audio_calls_clone.lock().unwrap().push(t.to_string());
            }
        };

        let b64_audio = base64::engine::general_purpose::STANDARD.encode(&[0u8; 8]);
        let stream = futures_util::stream::iter(vec![
            // An audio delta frame — must reach audio_handler.
            Ok(Message::Text(
                serde_json::json!({
                    "type": "response.audio.delta",
                    "delta": b64_audio,
                })
                .to_string()
                .into(),
            )),
            // A non-audio frame — must ALSO reach audio_handler (it's a no-op).
            Ok(Message::Text(
                serde_json::json!({ "type": "response.done", "response": {} })
                    .to_string()
                    .into(),
            )),
        ]);
        run_read_loop(
            stream,
            Arc::new(OpenAiRealtime::new()) as Arc<dyn RealtimeProvider>,
            write_tx,
            cost,
            Arc::new(OkRecorder) as Arc<dyn RecorderAdapter>,
            Arc::new(NoopDispatcher) as Arc<dyn DispatcherSeam>,
            sd_rx,
            emit,
            Arc::new(TokioMutex::new(None)),
            TEST_GENERATION,
            test_counter(),
            audio_handler,
            Arc::new(AtomicBool::new(true)), // mic always running in tests
            |_| {}, // no-op stop_audio (no device in test)
            None, // no write task to abort in unit tests
            |_month: u32, _used: u64, _budget: BudgetConfig| {},
            |_call_id: &str, _tool: &str| {},
        )
        .await;

        let calls = audio_calls.lock().unwrap();
        // Both text frames must have been forwarded to audio_handler.
        assert_eq!(calls.len(), 2, "expected 2 audio_handler calls, got {}", calls.len());
        assert_eq!(calls[0], "response.audio.delta");
        assert_eq!(calls[1], "response.done");
    }

    /// Verifies that when the cpal error_callback fires (mic_running goes false),
    /// the read loop detects it via the interval poll, emits "mic device lost",
    /// and exits fail-closed without a trailing idle.
    #[tokio::test]
    async fn mic_device_lost_emits_error_and_exits() {
        let cost = Arc::new(TokioMutex::new(CostTracker::new(
            BudgetConfig { enabled: false, monthly_limit_nanodollars: 0 },
            current_yyyymm(),
        )));
        let (write_tx, _rx) = mpsc::channel::<Message>(8);
        let (_sd, sd_rx) = oneshot::channel();
        let (log, emit) = collect_emit();
        // Simulate the cpal error_callback by setting the flag to false upfront.
        // The poll interval is 100ms but the stream is empty so the loop will pick
        // it up on the first poll tick.
        let mic_running = Arc::new(AtomicBool::new(false));
        // Track whether stop_audio was called (the mic-loss path must invoke it).
        let stop_audio_called = Arc::new(AtomicBool::new(false));
        let sac = Arc::clone(&stop_audio_called);
        run_read_loop(
            frame_stream(vec![]),
            Arc::new(OpenAiRealtime::new()) as Arc<dyn RealtimeProvider>,
            write_tx,
            cost,
            Arc::new(OkRecorder) as Arc<dyn RecorderAdapter>,
            Arc::new(NoopDispatcher) as Arc<dyn DispatcherSeam>,
            sd_rx,
            emit,
            Arc::new(TokioMutex::new(None)),
            TEST_GENERATION,
            test_counter(),
            |_| {}, // no-op audio_handler (no device in test)
            mic_running,
            move |_| { sac.store(true, Ordering::SeqCst); },
            None, // no write task to abort in unit tests
            |_month: u32, _used: u64, _budget: BudgetConfig| {},
            |_call_id: &str, _tool: &str| {},
        )
        .await;
        let events = log.lock().unwrap();
        assert!(
            events.iter().any(|(s, e)| s == "error" && e.as_deref() == Some("mic device lost")),
            "expected mic device lost error, got: {events:?}"
        );
        assert!(
            stop_audio_called.load(Ordering::SeqCst),
            "stop_audio must be called on mic-loss exit"
        );
        // error is terminal — no trailing idle that would clear the reason in the UI.
        assert!(!events.iter().any(|(s, _)| s == "idle"));
    }

    /// Verifies that stop_audio is invoked on a budget-exceeded exit so the cpal
    /// mic capture stops fail-closed even without an explicit stop_session call.
    #[tokio::test]
    async fn budget_trip_calls_stop_audio() {
        let cost = Arc::new(TokioMutex::new(CostTracker::new(
            BudgetConfig { enabled: true, monthly_limit_nanodollars: NANODOLLARS_PER_USD / 1_000_000 },
            current_yyyymm(),
        )));
        let (write_tx, _rx) = mpsc::channel::<Message>(8);
        let (_sd, sd_rx) = oneshot::channel();
        let (log, emit) = collect_emit();
        let stop_called = Arc::new(AtomicBool::new(false));
        let sc = Arc::clone(&stop_called);
        let frames = vec![serde_json::json!({
            "type": "response.done",
            "response": { "usage": { "input_token_details": { "audio_tokens": 1_000_000 } } }
        })];
        run_read_loop(
            frame_stream(frames),
            Arc::new(OpenAiRealtime::new()) as Arc<dyn RealtimeProvider>,
            write_tx,
            cost,
            Arc::new(OkRecorder) as Arc<dyn RecorderAdapter>,
            Arc::new(NoopDispatcher) as Arc<dyn DispatcherSeam>,
            sd_rx,
            emit,
            Arc::new(TokioMutex::new(None)),
            TEST_GENERATION,
            test_counter(),
            |_| {},
            Arc::new(AtomicBool::new(true)),
            move |_| { sc.store(true, Ordering::SeqCst); },
            None, // no write task to abort in unit tests
            |_month: u32, _used: u64, _budget: BudgetConfig| {},
            |_call_id: &str, _tool: &str| {},
        )
        .await;
        let events = log.lock().unwrap();
        assert!(events.iter().any(|(s, e)| s == "error" && e.as_deref() == Some("monthly budget exceeded")));
        assert!(
            stop_called.load(Ordering::SeqCst),
            "stop_audio must be called on budget-exceeded exit"
        );
    }

    /// Verifies that oversized text frames are dropped (DoS guard) and do not cause
    /// a panic or stop the session — the loop continues with the next frame.
    #[tokio::test]
    async fn oversized_text_frame_is_dropped_loop_continues() {
        let disp = Arc::new(RecordingDispatcher { calls: StdMutex::new(Vec::new()) });
        let cost = Arc::new(TokioMutex::new(CostTracker::new(
            BudgetConfig { enabled: false, monthly_limit_nanodollars: 0 },
            current_yyyymm(),
        )));
        let (write_tx, _rx) = mpsc::channel::<Message>(8);
        let (_sd, sd_rx) = oneshot::channel();
        let (_log, emit) = collect_emit();
        // One frame that exceeds MAX_WS_TEXT_BYTES, followed by a valid dispatch frame.
        use crate::audio_bridge::MAX_WS_TEXT_BYTES;
        let oversized_text = "X".repeat(MAX_WS_TEXT_BYTES + 1);
        let stream = futures_util::stream::iter(vec![
            Ok(Message::Text(oversized_text.into())),
            Ok(Message::Text(
                serde_json::json!({
                    "type": "response.function_call_arguments.done",
                    "call_id": "ok", "name": "write_note", "arguments": "{}"
                })
                .to_string()
                .into(),
            )),
        ]);
        run_read_loop(
            stream,
            Arc::new(OpenAiRealtime::new()) as Arc<dyn RealtimeProvider>,
            write_tx,
            cost,
            Arc::new(OkRecorder) as Arc<dyn RecorderAdapter>,
            disp.clone() as Arc<dyn DispatcherSeam>,
            sd_rx,
            emit,
            Arc::new(TokioMutex::new(None)),
            TEST_GENERATION,
            test_counter(),
            |_| {},
            Arc::new(AtomicBool::new(true)),
            |_| {},
            None, // no write task to abort in unit tests
            |_month: u32, _used: u64, _budget: BudgetConfig| {},
            |_call_id: &str, _tool: &str| {},
        )
        .await;
        // The oversized frame was dropped but the following valid dispatch fired.
        assert_eq!(disp.calls.lock().unwrap().as_slice(), ["write_note"]);
    }

    /// Verifies that a function-call with oversized arguments is dropped (DoS guard)
    /// and does not dispatch the tool.
    #[tokio::test]
    async fn oversized_args_frame_is_dropped() {
        use crate::audio_bridge::MAX_ARGS_LEN;
        let disp = Arc::new(RecordingDispatcher { calls: StdMutex::new(Vec::new()) });
        let cost = Arc::new(TokioMutex::new(CostTracker::new(
            BudgetConfig { enabled: false, monthly_limit_nanodollars: 0 },
            current_yyyymm(),
        )));
        let (write_tx, _rx) = mpsc::channel::<Message>(8);
        let (_sd, sd_rx) = oneshot::channel();
        let (_log, emit) = collect_emit();
        // Build a frame whose `arguments` string exceeds MAX_ARGS_LEN.
        let huge_args = "A".repeat(MAX_ARGS_LEN + 1);
        let stream = futures_util::stream::iter(vec![
            Ok(Message::Text(
                serde_json::json!({
                    "type": "response.function_call_arguments.done",
                    "call_id": "big", "name": "write_note", "arguments": huge_args
                })
                .to_string()
                .into(),
            )),
        ]);
        run_read_loop(
            stream,
            Arc::new(OpenAiRealtime::new()) as Arc<dyn RealtimeProvider>,
            write_tx,
            cost,
            Arc::new(OkRecorder) as Arc<dyn RecorderAdapter>,
            disp.clone() as Arc<dyn DispatcherSeam>,
            sd_rx,
            emit,
            Arc::new(TokioMutex::new(None)),
            TEST_GENERATION,
            test_counter(),
            |_| {},
            Arc::new(AtomicBool::new(true)),
            |_| {},
            None, // no write task to abort in unit tests
            |_month: u32, _used: u64, _budget: BudgetConfig| {},
            |_call_id: &str, _tool: &str| {},
        )
        .await;
        // Oversized args must be dropped — the tool must NOT be dispatched.
        assert!(
            disp.calls.lock().unwrap().is_empty(),
            "oversized-args frame must not dispatch the tool"
        );
    }

    // ── P1: run_read_loop abnormal exit aborts the writer handle ─────────────

    /// P1 regression test: proves that an abnormal exit (budget trip / WS error /
    /// timeout / mic lost) aborts the WS write task via the injected AbortHandle,
    /// AND calls stop_audio.
    ///
    /// We simulate an abnormal exit by providing a `mic_running = false` flag
    /// (same as `mic_device_lost_emits_error_and_exits` but now with a real
    /// AbortHandle to verify it gets aborted).
    ///
    /// The test:
    /// 1. Spawns a long-running "writer" task (sleeps for 10 s to simulate a
    ///    write-blocked task with queued PCM).
    /// 2. Extracts the AbortHandle and passes it into run_read_loop.
    /// 3. Provides mic_running=false so the loop exits abnormally immediately.
    /// 4. After run_read_loop returns, asserts the writer task is finished
    ///    (the AbortHandle was called, the task was cancelled).
    /// 5. Also asserts stop_audio was called.
    #[tokio::test]
    async fn abnormal_exit_aborts_writer_and_calls_stop_audio() {
        let cost = Arc::new(TokioMutex::new(CostTracker::new(
            BudgetConfig { enabled: false, monthly_limit_nanodollars: 0 },
            current_yyyymm(),
        )));
        let (write_tx, _rx) = mpsc::channel::<Message>(8);
        let (_sd, sd_rx) = oneshot::channel();
        let (log, emit) = collect_emit();

        // Spawn a "writer" task that would run indefinitely (simulating a blocked
        // writer with queued PCM that must NOT be flushed after abnormal exit).
        let writer_handle = tokio::spawn(async {
            // Sleep long enough that the test would hang if not aborted.
            tokio::time::sleep(std::time::Duration::from_secs(10)).await;
        });
        let abort_handle = writer_handle.abort_handle();

        let stop_called = Arc::new(AtomicBool::new(false));
        let sc = Arc::clone(&stop_called);

        // mic_running=false → abnormal exit path.
        run_read_loop(
            frame_stream(vec![]),
            Arc::new(OpenAiRealtime::new()) as Arc<dyn RealtimeProvider>,
            write_tx,
            cost,
            Arc::new(OkRecorder) as Arc<dyn RecorderAdapter>,
            Arc::new(NoopDispatcher) as Arc<dyn DispatcherSeam>,
            sd_rx,
            emit,
            Arc::new(TokioMutex::new(None)),
            TEST_GENERATION,
            test_counter(),
            |_| {},
            Arc::new(AtomicBool::new(false)), // mic already lost → abnormal exit
            move |_| { sc.store(true, Ordering::SeqCst); },
            Some(abort_handle),
        |_month: u32, _used: u64, _budget: BudgetConfig| {},
        |_call_id: &str, _tool: &str| {},)
        .await;

        // Verify the emitted events show an abnormal exit.
        let events = log.lock().unwrap();
        assert!(
            events.iter().any(|(s, e)| s == "error" && e.as_deref() == Some("mic device lost")),
            "expected mic device lost error, got: {events:?}"
        );

        // stop_audio must have been called.
        assert!(
            stop_called.load(Ordering::SeqCst),
            "stop_audio must be called on abnormal exit"
        );

        // The writer task must be aborted.  Use a timeout join to detect hangs;
        // if the abort worked the join returns Err(JoinError::Cancelled).
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            writer_handle,
        ).await;
        assert!(
            result.is_ok(),
            "writer task must complete (be aborted) within 2s of abnormal run_read_loop exit"
        );
        // join returns Ok(Err(JoinError)) where the JoinError is Cancelled.
        let join_result = result.unwrap();
        assert!(
            join_result.is_err() && join_result.unwrap_err().is_cancelled(),
            "writer task must have been cancelled (aborted), not completed normally"
        );
    }

    /// Verifies that on a NORMAL server-close exit, the writer task is NOT aborted
    /// by run_read_loop (the channel close drains it instead).
    ///
    /// The writer task completes normally here; the AbortHandle is passed as
    /// `None` (matching the production call path for normal close where we pass
    /// `None` — actually in production we always pass Some, but the normal-close
    /// path does not call abort).  We test the observable behaviour: writer
    /// completes naturally.
    #[tokio::test]
    async fn normal_server_close_does_not_abort_writer() {
        let cost = Arc::new(TokioMutex::new(CostTracker::new(
            BudgetConfig { enabled: false, monthly_limit_nanodollars: 0 },
            current_yyyymm(),
        )));
        let (write_tx, mut write_rx) = mpsc::channel::<Message>(8);
        let (_sd, sd_rx) = oneshot::channel();
        let (_log, emit) = collect_emit();

        // A writer that simply drains the write_rx channel and records what it got.
        let wrote: Arc<std::sync::Mutex<Vec<String>>> = Arc::new(std::sync::Mutex::new(Vec::new()));
        let wrote2 = Arc::clone(&wrote);
        let writer_handle = tokio::spawn(async move {
            while let Some(msg) = write_rx.recv().await {
                if let Message::Text(t) = msg {
                    wrote2.lock().unwrap().push(t.to_string());
                }
            }
        });
        // We don't pass the abort handle for normal close (pass None to simulate
        // not aborting the writer on normal close path, though in production the
        // abort_handle is still passed but abort() is not called on this path).
        let abort_handle = writer_handle.abort_handle();

        // Normal server close: stream ends with Close message.
        let stream = futures_util::stream::iter(vec![Ok(Message::Close(None))]);
        run_read_loop(
            stream,
            Arc::new(OpenAiRealtime::new()) as Arc<dyn RealtimeProvider>,
            write_tx,
            cost,
            Arc::new(OkRecorder) as Arc<dyn RecorderAdapter>,
            Arc::new(NoopDispatcher) as Arc<dyn DispatcherSeam>,
            sd_rx,
            emit,
            Arc::new(TokioMutex::new(None)),
            TEST_GENERATION,
            test_counter(),
            |_| {},
            Arc::new(AtomicBool::new(true)),
            |_| {},
            None, // normal close: don't pass AbortHandle so writer runs to completion
            |_month: u32, _used: u64, _budget: BudgetConfig| {},
            |_call_id: &str, _tool: &str| {},
        )
        .await;

        // Drop the abort handle (not used here) to keep clippy happy.
        drop(abort_handle);

        // After normal close the writer drains its channel (write_tx was dropped
        // when run_read_loop exited) and finishes naturally.
        let _ = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            writer_handle,
        ).await.expect("writer task must finish naturally after normal close");
    }

    // ---- get_cost_snapshot / build_cost_snapshot (pull path, koe-9xi) -----

    #[test]
    fn build_cost_snapshot_absent_row_is_zero_spent() {
        // No ledger row yet this month (load -> None) means $0 spent, NOT an error.
        let budget = BudgetConfig {
            enabled: true,
            monthly_limit_nanodollars: 32 * NANODOLLARS_PER_USD,
        };
        let snap = build_cost_snapshot(&OkRecorder, &budget, 202605, 5)
            .expect("absent row is a valid $0 snapshot");
        assert_eq!(snap.used_nanodollars, 0);
        assert_eq!(snap.month, 202605);
        assert_eq!(snap.sequence, 5);
        assert_eq!(snap.limit_nanodollars, Some(32 * NANODOLLARS_PER_USD));
        assert!(!snap.over_budget);
    }

    #[test]
    fn build_cost_snapshot_uses_persisted_authoritative_total() {
        // The spend comes from the recorder's additive ledger (the authority); the
        // over_budget bool is judged on that u64 total via is_over.
        let (rec, persisted) = SharedSnapshotRecorder::new();
        persisted
            .lock()
            .unwrap()
            .insert(202605, 40 * NANODOLLARS_PER_USD);
        let budget = BudgetConfig {
            enabled: true,
            monthly_limit_nanodollars: 32 * NANODOLLARS_PER_USD,
        };
        let snap = build_cost_snapshot(&rec, &budget, 202605, 9).expect("ok");
        assert_eq!(snap.used_nanodollars, 40 * NANODOLLARS_PER_USD);
        assert!(snap.over_budget, "$40 of a $32 cap is over budget");
        assert_eq!(snap.remaining_usd, Some(0.0));
    }

    #[test]
    fn build_cost_snapshot_recorder_error_is_fail_closed() {
        // A recorder load failure must propagate as Err — the caller surfaces an
        // explicit "unknown" state, never a fabricated $0 that hides real spend.
        let budget = BudgetConfig::default();
        assert!(build_cost_snapshot(&ReadWriteFailRecorder, &budget, 202605, 0).is_err());
    }

    // ---- cost-update emit (push path, koe-9xi) ---------------------------

    type CostEmits = Arc<StdMutex<Vec<(u32, u64, BudgetConfig)>>>;

    /// Drives one frame through `handle_text` capturing every `emit_cost` call, so a
    /// test can assert the live `cost-update` payload (effective month, authoritative
    /// total, budget) without a socket / `AppHandle` (same AppHandle-free discipline
    /// as `drive_usage`, but with a recording cost emitter instead of a no-op).
    async fn drive_capturing_cost(
        frame: &Value,
        provider: &Arc<dyn RealtimeProvider>,
        recorder: &Arc<dyn RecorderAdapter>,
        cost: &Arc<TokioMutex<CostTracker>>,
    ) -> (LoopAction, CostEmits) {
        let emits: CostEmits = Arc::new(StdMutex::new(Vec::new()));
        let sink = Arc::clone(&emits);
        let (write_tx, _write_rx) = mpsc::channel::<Message>(8);
        let (rec_tx, _rec_rx) = mpsc::channel::<ConversationRecord>(8);
        let dispatcher: Arc<dyn DispatcherSeam> = Arc::new(NoopDispatcher);
        let mut dispatch_tasks = tokio::task::JoinSet::new();
        let mut save_failures = 0u32;
        let mut pending = PendingCost::default();
        let (mut cap_warned, mut journal_drop_warned) = (false, false);
        let action = handle_text(
            frame,
            provider,
            &write_tx,
            cost,
            recorder,
            &rec_tx,
            &dispatcher,
            &mut dispatch_tasks,
            &mut save_failures,
            &mut pending,
            &mut cap_warned,
            &mut journal_drop_warned,
            &move |mo: u32, us: u64, bg: BudgetConfig| sink.lock().unwrap().push((mo, us, bg)),
            &|_call_id: &str, _tool: &str| {},
        )
        .await;
        (action, emits)
    }

    #[tokio::test]
    async fn usage_frame_emits_cost_update_with_authoritative_total() {
        // A response.done usage frame pushes exactly ONE cost-update carrying the
        // effective accounting month, the authoritative (ledger) total, and the budget.
        let provider: Arc<dyn RealtimeProvider> = Arc::new(OpenAiRealtime::new());
        let month = current_yyyymm();
        let budget = BudgetConfig {
            enabled: true,
            monthly_limit_nanodollars: 100 * NANODOLLARS_PER_USD,
        };
        let cost = Arc::new(TokioMutex::new(CostTracker::new(budget, month)));
        // OkRecorder.add_month_cost echoes the delta as the new total (single session).
        let recorder: Arc<dyn RecorderAdapter> = Arc::new(OkRecorder);
        // 1_000_000 audio input tokens = $32.
        let usage = serde_json::json!({
            "type": "response.done",
            "response": { "usage": { "input_token_details": { "audio_tokens": 1_000_000u64 } } }
        });
        let (action, emits) = drive_capturing_cost(&usage, &provider, &recorder, &cost).await;
        assert!(matches!(action, LoopAction::Continue));
        let calls = emits.lock().unwrap().clone();
        assert_eq!(calls.len(), 1, "exactly one cost-update per usage frame");
        let (emit_month, emit_used, emit_budget) = calls[0];
        assert_eq!(emit_month, month);
        assert_eq!(emit_used, 32 * NANODOLLARS_PER_USD);
        assert_eq!(emit_budget, budget);
        // The snapshot the production closure would build from this is under budget.
        assert!(!CostSnapshot::new(emit_month, emit_used, &emit_budget, 0).over_budget);
    }

    #[tokio::test]
    async fn over_budget_usage_emits_snapshot_then_stops_fail_closed() {
        // Even when the usage trips the cap, the over-budget snapshot is emitted
        // BEFORE the loop stops, so the UI flips to the stop state (then the session
        // stops fail-closed).
        let provider: Arc<dyn RealtimeProvider> = Arc::new(OpenAiRealtime::new());
        let month = current_yyyymm();
        let budget = BudgetConfig {
            enabled: true,
            monthly_limit_nanodollars: NANODOLLARS_PER_USD, // $1 cap
        };
        let cost = Arc::new(TokioMutex::new(CostTracker::new(budget, month)));
        let recorder: Arc<dyn RecorderAdapter> = Arc::new(OkRecorder);
        // $32 >> $1 cap.
        let usage = serde_json::json!({
            "type": "response.done",
            "response": { "usage": { "input_token_details": { "audio_tokens": 1_000_000u64 } } }
        });
        let (action, emits) = drive_capturing_cost(&usage, &provider, &recorder, &cost).await;
        assert!(matches!(action, LoopAction::Stop("monthly budget exceeded")));
        let calls = emits.lock().unwrap().clone();
        assert_eq!(calls.len(), 1, "the over-budget snapshot is still emitted");
        let (m, used, b) = calls[0];
        assert!(CostSnapshot::new(m, used, &b, 0).over_budget);
    }

    #[tokio::test]
    async fn non_usage_frame_does_not_emit_cost_update() {
        // A non-usage frame (no spend) must not push a cost-update.
        let provider: Arc<dyn RealtimeProvider> = Arc::new(OpenAiRealtime::new());
        let cost = Arc::new(TokioMutex::new(CostTracker::new(
            BudgetConfig::default(),
            current_yyyymm(),
        )));
        let recorder: Arc<dyn RecorderAdapter> = Arc::new(OkRecorder);
        let frame = serde_json::json!({ "type": "response.created" });
        let (_action, emits) = drive_capturing_cost(&frame, &provider, &recorder, &cost).await;
        assert!(
            emits.lock().unwrap().is_empty(),
            "no cost-update for a non-usage frame"
        );
    }

    #[tokio::test]
    async fn add_failure_with_readback_does_not_emit_nondurable_total() {
        // Codex Cloud P2: when add_month_cost fails but the readback succeeds, the
        // total carries non-durable (unpersisted) spend. Emitting it would let a
        // later get_cost_snapshot pull (reading only the persisted ledger, a LOWER
        // value) mint a higher sequence and overwrite an over-budget display, hiding
        // the stop state. So the non-durable path must NOT emit. The budget gate
        // still stops fail-closed on the lower bound; here the cap is high enough that
        // the loop continues, isolating the no-emit behavior.
        let provider: Arc<dyn RealtimeProvider> = Arc::new(OpenAiRealtime::new());
        let month = current_yyyymm();
        let budget = BudgetConfig {
            enabled: true,
            monthly_limit_nanodollars: 1000 * NANODOLLARS_PER_USD,
        };
        let cost = Arc::new(TokioMutex::new(CostTracker::new(budget, month)));
        // add_month_cost -> Err, load_cost_snapshot -> Ok(None): one failure leaves
        // save_failures = 1 (< MAX), readback succeeds, total = 0 + delta (non-durable).
        let recorder: Arc<dyn RecorderAdapter> = Arc::new(FailingRecorder);
        let usage = serde_json::json!({
            "type": "response.done",
            "response": { "usage": { "input_token_details": { "audio_tokens": 1_000_000u64 } } }
        });
        let (action, emits) = drive_capturing_cost(&usage, &provider, &recorder, &cost).await;
        assert!(
            matches!(action, LoopAction::Continue),
            "under-cap non-durable add-failure continues"
        );
        assert!(
            emits.lock().unwrap().is_empty(),
            "no cost-update for a non-durable (unpersisted) total"
        );
    }
}
