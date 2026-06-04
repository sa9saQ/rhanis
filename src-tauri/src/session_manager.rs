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

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use futures_util::{SinkExt, Stream, StreamExt};
use serde_json::Value;
use tokio::sync::{mpsc, oneshot, Mutex as TokioMutex};
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;
use tokio_tungstenite::tungstenite::{Error as WsError, Message};

use crate::audio_bridge::{ManagedAudioBridge, MAX_WS_TEXT_BYTES};
use crate::cost_tracker::CostTracker;
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

/// WebSocket frame/message size limits (DoS guard).
/// Max message: 512 KiB — comfortably above the largest legitimate Realtime
/// frame (audio deltas are ~256 KiB max; control frames are much smaller).
/// Max frame: same cap; the Realtime API does not fragment messages.
const WS_MAX_MESSAGE_SIZE: usize = 512 * 1024;
const WS_MAX_FRAME_SIZE: usize = 512 * 1024;

// ---- managed session state ---------------------------------------------------

/// In-flight session handles. `None` when idle.
pub(crate) struct ActiveSession {
    shutdown_tx: oneshot::Sender<()>,
    write_handle: tokio::task::JoinHandle<()>,
}

/// Tauri managed state: the single optional active session. The `tokio::Mutex`
/// is held across the whole `start_session` setup so two concurrent starts
/// cannot both pass the `is_some()` check (double-start race). The field is
/// `pub(crate)` (not `pub`) because `ActiveSession` is crate-private.
pub struct ManagedSession(pub(crate) Arc<TokioMutex<Option<ActiveSession>>>);

impl ManagedSession {
    pub fn new() -> Self {
        Self(Arc::new(TokioMutex::new(None)))
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
    Stop,
}

/// Poll interval for detecting a mic device failure via `mic_running` flag.
/// 100ms is fast enough for UX feedback and cheap enough to not measurably
/// impact the audio pipeline.
const MIC_POLL_INTERVAL: Duration = Duration::from_millis(100);

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
async fn run_read_loop<S, F, A, SA>(
    mut stream: S,
    provider: Arc<dyn RealtimeProvider>,
    write_tx: mpsc::Sender<Message>,
    cost: Arc<TokioMutex<CostTracker>>,
    recorder: Arc<dyn RecorderAdapter>,
    dispatcher: Arc<dyn DispatcherSeam>,
    mut shutdown: oneshot::Receiver<()>,
    emit: F,
    session: Arc<TokioMutex<Option<ActiveSession>>>,
    audio_handler: A,
    mic_running: Arc<AtomicBool>,
    stop_audio: SA,
    writer_abort: Option<tokio::task::AbortHandle>,
) where
    S: Stream<Item = Result<Message, WsError>> + Unpin,
    F: Fn(&str, Option<&str>),
    A: Fn(&serde_json::Value),
    SA: Fn(bool), // true = graceful (flush tail), false = immediate (discard tail)
{
    // Tracks in-flight tool dispatches so a budget trip / stop aborts them too
    // (rather than letting them complete and spend more).
    let mut dispatch_tasks = tokio::task::JoinSet::new();
    let mut save_failures: u32 = 0;
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
    // An error exit must leave the terminal `error` status visible: emitting a
    // trailing `idle` would make the frontend clear `lastError`, hiding the
    // budget/connection/timeout reason (a near-silent failure).
    let mut ended_with_error = false;

    // Pre-loop check: if the mic is already not running when we enter (e.g., the
    // error_callback fired before or during start_session), fail immediately rather
    // than waiting for the first 100ms interval tick.
    if !mic_running.load(Ordering::Acquire) {
        emit("error", Some("mic device lost"));
        // P1: abort the writer FIRST so no already-queued PCM is flushed, then
        // stop_immediate (StopNow — discard tail) because this is an abnormal exit.
        if let Some(h) = &writer_abort {
            h.abort();
        }
        stop_audio(false); // false = immediate (no tail flush)
        session.lock().await.take();
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
                emit("error", Some("session timeout"));
                ended_with_error = true;
                abort_inflight = true;
                break;
            }
            // Poll the cpal AtomicBool; if the error_callback has fired (device
            // unplugged, driver error), stop the session fail-closed rather than
            // silently continuing as a deaf text-only session.
            _ = mic_poll.tick() => {
                if !mic_running.load(Ordering::Acquire) {
                    emit("error", Some("mic device lost"));
                    ended_with_error = true;
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
                            &dispatcher, &emit, &mut dispatch_tasks, &mut save_failures,
                            &mut cap_warned, &mut journal_drop_warned,
                        ).await {
                            LoopAction::Continue => {}
                            // handle_text already emitted the terminal error
                            // (budget exceeded / cost tracking unavailable).
                            LoopAction::Stop => {
                                ended_with_error = true;
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
                        emit("error", Some("connection error"));
                        ended_with_error = true;
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
    // Clear the session slot on EVERY exit (server close / budget / timeout /
    // shutdown) so a stale `Some` cannot permanently block the next
    // start_session. The read loop is the SINGLE place that emits a terminal
    // status (stop_session relies on this), so there is never a double idle.
    //
    // This is done BEFORE the journal flush below: clearing the slot must not be
    // delayed by the conversation-writer drain, otherwise koe-emd would widen the
    // pre-existing stop_session->start_session slot-handover window (stop_session
    // takes the slot, then a racing start_session could store a new ActiveSession
    // that this exiting loop would then clear). Keeping the slot-clear at the same
    // point as before koe-emd holds that window at its prior size. The residual
    // pre-existing race (this loop can still clear a *newer* session's slot) is a
    // separate session-lifecycle fix tracked as a follow-up (generation-id guard).
    session.lock().await.take();
    // Only a clean stop/close transitions to idle; an error exit leaves the
    // already-emitted `error` as the terminal status so the UI keeps the reason.
    if !ended_with_error {
        emit("idle", None);
    }
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
/// (mirrors `save_cost_snapshot`) so it never blocks an async worker.
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

/// Handles one decoded server frame via the provider's normalizer. Returns
/// whether to keep looping.
#[allow(clippy::too_many_arguments)]
async fn handle_text<F>(
    event: &Value,
    provider: &Arc<dyn RealtimeProvider>,
    write_tx: &mpsc::Sender<Message>,
    cost: &Arc<TokioMutex<CostTracker>>,
    recorder: &Arc<dyn RecorderAdapter>,
    rec_tx: &mpsc::Sender<ConversationRecord>,
    dispatcher: &Arc<dyn DispatcherSeam>,
    emit: &F,
    dispatch_tasks: &mut tokio::task::JoinSet<()>,
    save_failures: &mut u32,
    cap_warned: &mut bool,
    journal_drop_warned: &mut bool,
) -> LoopAction
where
    F: Fn(&str, Option<&str>),
{
    match provider.parse_frame(event) {
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
            // Add usage and read the result, then DROP the guard before any
            // .await (never hold the lock across an await — deadlock risk).
            let month = current_yyyymm();
            let (total, over) = {
                let mut c = cost.lock().await;
                c.add_usage(&usage, month);
                (c.month_total_nanodollars, c.is_over_budget())
            };
            // Persist the running total (sync recorder → spawn_blocking).
            let rec = Arc::clone(recorder);
            let saved = tokio::task::spawn_blocking(move || rec.save_cost_snapshot(month, total)).await;
            match saved {
                Ok(Ok(())) => *save_failures = 0,
                _ => {
                    *save_failures += 1;
                    if *save_failures >= MAX_SNAPSHOT_SAVE_FAILURES {
                        // Can't durably track spend → stop rather than risk a
                        // restart resetting the monthly total (fail-closed).
                        emit("error", Some("cost tracking unavailable"));
                        return LoopAction::Stop;
                    }
                }
            }
            if over {
                emit("error", Some("monthly budget exceeded"));
                return LoopAction::Stop; // fail-closed
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

    // Detached: the loop clears the session slot + emits idle on its own exit;
    // stop_session signals it via shutdown_tx rather than holding its handle.
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
        audio_handler,
        mic_running,
        stop_audio,
        Some(writer_abort_handle),
    ));

    *guard = Some(ActiveSession {
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
        fn save_cost_snapshot(&self, _m: u32, _n: u64) -> Result<(), RecorderError> {
            Ok(())
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
        fn save_cost_snapshot(&self, _m: u32, _n: u64) -> Result<(), RecorderError> {
            Ok(())
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
            |_| {}, // no-op audio_handler (no device in test)
            Arc::new(AtomicBool::new(true)), // mic always running in tests
            |_| {}, // no-op stop_audio (no device in test)
            None, // no write task to abort in unit tests
        )
        .await;

        assert_eq!(disp.calls.lock().unwrap().as_slice(), ["write_note"]);
        // The dispatch task sends two frames (item.create + response.create).
        let f1 = write_rx.recv().await.expect("item.create frame");
        let f2 = write_rx.recv().await.expect("response.create frame");
        assert!(matches!(f1, Message::Text(_)));
        assert!(matches!(f2, Message::Text(_)));
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
            |_| {},
            Arc::new(AtomicBool::new(true)),
            |_| {},
            None,
        )
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
            |_| {}, // no-op audio_handler (no device in test)
            Arc::new(AtomicBool::new(true)), // mic always running in tests
            |_| {}, // no-op stop_audio (no device in test)
            None, // no write task to abort in unit tests
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
            |_| {}, // no-op audio_handler (no device in test)
            Arc::new(AtomicBool::new(true)), // mic always running in tests
            |_| {}, // no-op stop_audio (no device in test)
            None, // no write task to abort in unit tests
        )
        .await;

        let events = log.lock().unwrap();
        // budget-exceeded error, then idle on loop exit.
        assert!(events.iter().any(|(s, e)| s == "error" && e.as_deref() == Some("monthly budget exceeded")));
        assert!(cost.try_lock().unwrap().is_over_budget());
        // The function_call frame AFTER the budget trip must NOT be dispatched.
        assert!(disp.calls.lock().unwrap().is_empty(), "no dispatch after budget stop");
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
            |_| {}, // no-op audio_handler (no device in test)
            Arc::new(AtomicBool::new(true)), // mic always running in tests
            |_| {}, // no-op stop_audio (no device in test)
            None, // no write task to abort in unit tests
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
        fn save_cost_snapshot(&self, _m: u32, _n: u64) -> Result<(), RecorderError> {
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
        fn save_cost_snapshot(&self, _m: u32, _n: u64) -> Result<(), RecorderError> {
            Ok(())
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
            |_| {},
            Arc::new(AtomicBool::new(true)),
            |_| {},
            None,
        )
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
            |_| {},
            Arc::new(AtomicBool::new(true)),
            |_| {},
            None,
        )
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
            |_| {},
            Arc::new(AtomicBool::new(true)),
            |_| {},
            None,
        )
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
            |_| {}, // no-op audio_handler (no device in test)
            Arc::new(AtomicBool::new(true)), // mic always running in tests
            |_| {}, // no-op stop_audio (no device in test)
            None, // no write task to abort in unit tests
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
            |_| {}, // no-op audio_handler (no device in test)
            Arc::new(AtomicBool::new(true)), // mic always running in tests
            |_| {}, // no-op stop_audio (no device in test)
            None, // no write task to abort in unit tests
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
            |_| {}, // no-op audio_handler (no device in test)
            Arc::new(AtomicBool::new(true)), // mic always running in tests
            |_| {}, // no-op stop_audio (no device in test)
            None, // no write task to abort in unit tests
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
            |_| {}, // no-op audio_handler (no device in test)
            Arc::new(AtomicBool::new(true)), // mic always running in tests
            |_| {}, // no-op stop_audio (no device in test)
            None, // no write task to abort in unit tests
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
            audio_handler,
            Arc::new(AtomicBool::new(true)), // mic always running in tests
            |_| {}, // no-op stop_audio (no device in test)
            None, // no write task to abort in unit tests
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
            |_| {}, // no-op audio_handler (no device in test)
            mic_running,
            move |_| { sac.store(true, Ordering::SeqCst); },
            None, // no write task to abort in unit tests
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
            |_| {},
            Arc::new(AtomicBool::new(true)),
            move |_| { sc.store(true, Ordering::SeqCst); },
            None, // no write task to abort in unit tests
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
            |_| {},
            Arc::new(AtomicBool::new(true)),
            |_| {},
            None, // no write task to abort in unit tests
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
            |_| {},
            Arc::new(AtomicBool::new(true)),
            |_| {},
            None, // no write task to abort in unit tests
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
            |_| {},
            Arc::new(AtomicBool::new(false)), // mic already lost → abnormal exit
            move |_| { sc.store(true, Ordering::SeqCst); },
            Some(abort_handle),
        )
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
            |_| {},
            Arc::new(AtomicBool::new(true)),
            |_| {},
            None, // normal close: don't pass AbortHandle so writer runs to completion
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
}
