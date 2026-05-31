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

use std::fmt;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use futures_util::{SinkExt, Stream, StreamExt};
use serde_json::Value;
use tokio::sync::{mpsc, oneshot, Mutex as TokioMutex};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::{Error as WsError, Message};

use crate::cost_tracker::{CostTracker, Usage};
use crate::events::{ManagedSequenceCounter, SequenceCounter};
use crate::realtime_types::{DispatcherSeam, FunctionCall, ManagedDispatcher, ToolSchema};
use crate::secret_store::{ManagedSecretStore, SecretString, OPENAI_KEY_NAME};
use crate::settings_store::ManagedSettings;
use crate::storage::adapter::{ManagedRecorder, RecorderAdapter};

const REALTIME_URL: &str = "wss://api.openai.com/v1/realtime?model=gpt-realtime-2";
/// Hard session cap (also a coarse cost backstop). Mirrors CLAUDE.md's 30 min.
const SESSION_TIMEOUT: Duration = Duration::from_secs(30 * 60);
const WRITE_CHANNEL_CAP: usize = 32;
/// Consecutive cost-snapshot save failures tolerated before stopping fail-closed
/// (a persistent failure means a restart could lose the running total).
const MAX_SNAPSHOT_SAVE_FAILURES: u32 = 3;

// ---- RealtimeAuth ------------------------------------------------------------

/// The connection credential. `Byok` is M1; `ManagedCredit` is a stub for M4
/// (operator-key prepaid). NOT `Serialize`/`Clone`; `Debug` is redacted so the
/// key cannot leak through a derived format.
pub enum RealtimeAuth {
    Byok(SecretString),
    /// M4 operator-key path; unused in M1.
    #[allow(dead_code)]
    ManagedCredit { token: SecretString },
}

impl RealtimeAuth {
    /// Builds the `Authorization: Bearer …` header. `expose()` is called ONLY
    /// here; the formatted string lives in this frame and drops immediately.
    fn bearer_header(&self) -> Result<HeaderValue, &'static str> {
        let secret = match self {
            RealtimeAuth::Byok(s) => s,
            RealtimeAuth::ManagedCredit { token } => token,
        };
        // Zeroize the intermediate "Bearer …" string so the key does not linger
        // in a second heap allocation after HeaderValue copies it. (The
        // HeaderValue's own bytes + the TLS write buffer remain an unavoidable
        // minimum exposure window for a BYOK desktop client.)
        let mut bearer = zeroize::Zeroizing::new(String::with_capacity(7 + secret.expose().len()));
        bearer.push_str("Bearer ");
        bearer.push_str(secret.expose());
        HeaderValue::from_str(&bearer).map_err(|_| "invalid credential")
    }
}

impl fmt::Debug for RealtimeAuth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RealtimeAuth::Byok(_) => f.write_str("RealtimeAuth::Byok(***)"),
            RealtimeAuth::ManagedCredit { .. } => f.write_str("RealtimeAuth::ManagedCredit(***)"),
        }
    }
}

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

/// Best-effort parse of an OpenAI Realtime usage payload into [`Usage`].
///
/// NOTE: the exact token-detail field names are confirmed against live traffic
/// in koe-ef8 (Windows E2E). Unknown fields default to 0, so an unexpected shape
/// under-counts rather than panicking — the session timeout is the backstop.
fn parse_usage(event: &Value) -> Option<Usage> {
    let u = event.get("response")?.get("usage")?;
    let input = u.get("input_token_details");
    let output = u.get("output_token_details");
    let get = |d: Option<&Value>, k: &str| -> u64 {
        d.and_then(|d| d.get(k)).and_then(Value::as_u64).unwrap_or(0)
    };
    Some(Usage {
        audio_input_tokens: get(input, "audio_tokens"),
        text_input_tokens: get(input, "text_tokens"),
        cached_input_tokens: get(input, "cached_tokens"),
        audio_output_tokens: get(output, "audio_tokens"),
        text_output_tokens: get(output, "text_tokens"),
    })
}

/// Builds the `session.update` frame advertising the dispatcher's tools.
fn build_session_update(tools: &[ToolSchema]) -> Value {
    serde_json::json!({
        "type": "session.update",
        "session": { "tools": tools, "tool_choice": "auto" }
    })
}

// ---- read loop (AppHandle-free; unit-tested via injected frames + emit) ------

/// What the loop should do after handling one frame.
enum LoopAction {
    Continue,
    Stop,
}

/// The session read loop. Generic over the frame source `S` and an `emit`
/// closure `F` so it runs with no live socket and no `AppHandle` in tests.
#[allow(clippy::too_many_arguments)]
async fn run_read_loop<S, F>(
    mut stream: S,
    write_tx: mpsc::Sender<Message>,
    cost: Arc<TokioMutex<CostTracker>>,
    recorder: Arc<dyn RecorderAdapter>,
    dispatcher: Arc<dyn DispatcherSeam>,
    mut shutdown: oneshot::Receiver<()>,
    emit: F,
    session: Arc<TokioMutex<Option<ActiveSession>>>,
) where
    S: Stream<Item = Result<Message, WsError>> + Unpin,
    F: Fn(&str, Option<&str>),
{
    // Tracks in-flight tool dispatches so a budget trip / stop aborts them too
    // (rather than letting them complete and spend more).
    let mut dispatch_tasks = tokio::task::JoinSet::new();
    let mut save_failures: u32 = 0;
    let deadline = tokio::time::sleep(SESSION_TIMEOUT);
    tokio::pin!(deadline);

    // Whether to abort in-flight tool dispatches on exit. A *deliberate* stop
    // (shutdown / budget trip / timeout / connection error) aborts them so none
    // completes and spends after the decision to stop. A *normal* server close
    // drains them instead, so their side effects (e.g. a note write) and final
    // response frames complete rather than being killed mid-flight.
    let abort_inflight: bool;
    loop {
        tokio::select! {
            _ = &mut shutdown => {
                abort_inflight = true;
                break;
            }
            _ = &mut deadline => {
                emit("error", Some("session timeout"));
                abort_inflight = true;
                break;
            }
            frame = stream.next() => {
                match frame {
                    Some(Ok(Message::Text(txt))) => {
                        match handle_text(
                            txt.as_str(), &write_tx, &cost, &recorder, &dispatcher,
                            &emit, &mut dispatch_tasks, &mut save_failures,
                        ).await {
                            LoopAction::Continue => {}
                            LoopAction::Stop => {
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
                    // Binary/ping/pong/frame — audio is handled elsewhere (audio_bridge).
                    Some(Ok(_)) => {}
                    Some(Err(_)) => {
                        emit("error", Some("connection error"));
                        abort_inflight = true;
                        break;
                    }
                }
            }
        }
    }

    if abort_inflight {
        dispatch_tasks.abort_all();
    } else {
        // Drain in-flight dispatches so their side effects + final frames finish.
        while dispatch_tasks.join_next().await.is_some() {}
    }
    // Clear the session slot on EVERY exit (server close / budget / timeout /
    // shutdown) so a stale `Some` cannot permanently block the next
    // start_session. The read loop is the SINGLE place that emits the terminal
    // idle (stop_session relies on this), so there is never a double transition.
    session.lock().await.take();
    emit("idle", None);
}

/// Handles one decoded server text frame. Returns whether to keep looping.
#[allow(clippy::too_many_arguments)]
async fn handle_text<F>(
    txt: &str,
    write_tx: &mpsc::Sender<Message>,
    cost: &Arc<TokioMutex<CostTracker>>,
    recorder: &Arc<dyn RecorderAdapter>,
    dispatcher: &Arc<dyn DispatcherSeam>,
    emit: &F,
    dispatch_tasks: &mut tokio::task::JoinSet<()>,
    save_failures: &mut u32,
) -> LoopAction
where
    F: Fn(&str, Option<&str>),
{
    let event: Value = match serde_json::from_str(txt) {
        Ok(v) => v,
        Err(_) => return LoopAction::Continue, // ignore unparseable frames
    };

    match event.get("type").and_then(Value::as_str) {
        Some("response.function_call_arguments.done") => {
            let call_id = event
                .get("call_id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let name = event
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            // `arguments` arrives as a JSON-encoded string; parse it, defaulting
            // to null so a malformed blob still reaches the tool (which validates).
            let args = event
                .get("arguments")
                .and_then(Value::as_str)
                .and_then(|s| serde_json::from_str(s).ok())
                .unwrap_or(Value::Null);

            let dispatcher = Arc::clone(dispatcher);
            let tx = write_tx.clone();
            dispatch_tasks.spawn(async move {
                let result = dispatcher.dispatch(FunctionCall { call_id, name, args }).await;
                // Bounded channel: if the writer is gone (session stopped) these
                // simply fail and the task ends.
                let _ = tx.send(Message::Text(result.conversation_item_create.to_string().into())).await;
                let _ = tx.send(Message::Text(result.response_create.to_string().into())).await;
            });
            LoopAction::Continue
        }
        Some("response.done") => {
            let Some(usage) = parse_usage(&event) else {
                return LoopAction::Continue;
            };
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
        _ => LoopAction::Continue,
    }
}

// ---- Tauri commands ----------------------------------------------------------

/// Builds the WS upgrade request with the auth + beta headers.
fn build_request(
    auth: &RealtimeAuth,
) -> Result<tokio_tungstenite::tungstenite::handshake::client::Request, &'static str> {
    let mut request = REALTIME_URL
        .into_client_request()
        .map_err(|_| "invalid realtime url")?;
    let headers = request.headers_mut();
    headers.insert("Authorization", auth.bearer_header()?);
    headers.insert("OpenAI-Beta", HeaderValue::from_static("realtime=v1"));
    Ok(request)
}

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

    let request = build_request(&auth).map_err(|e| e.to_string())?;
    drop(auth); // the credential must not outlive header construction

    let (ws_stream, _resp) = tokio_tungstenite::connect_async(request).await.map_err(|_| {
        emit_session_status(&app, &seq.0, "error", Some("connection failed"));
        "connection failed".to_string()
    })?;
    emit_session_status(&app, &seq.0, "connected", None);

    let (mut sink, stream) = ws_stream.split();

    // Advertise tools so the model can issue function calls (else the dispatch
    // loop is permanently idle).
    let session_update = build_session_update(&dispatcher.0.tool_schemas());
    sink.send(Message::Text(session_update.to_string().into()))
        .await
        .map_err(|_| "session.update failed".to_string())?;

    // Single writer owns the sink → concurrent dispatch tasks can't interleave.
    let (write_tx, mut write_rx) = mpsc::channel::<Message>(WRITE_CHANNEL_CAP);
    let write_handle = tokio::spawn(async move {
        while let Some(msg) = write_rx.recv().await {
            if sink.send(msg).await.is_err() {
                break;
            }
        }
    });

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
    // Detached: the loop clears the session slot + emits idle on its own exit;
    // stop_session signals it via shutdown_tx rather than holding its handle.
    tokio::spawn(run_read_loop(
        stream,
        write_tx,
        cost,
        recorder_arc,
        dispatcher_arc,
        shutdown_rx,
        emit,
        session_for_loop,
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
pub async fn stop_session(session: tauri::State<'_, ManagedSession>) -> Result<(), String> {
    let taken = { session.0.lock().await.take() };
    if let Some(active) = taken {
        // Signal the read loop to break; it clears the (now-empty) slot and emits
        // the single terminal idle on exit. Abort the writer so it stops promptly.
        // We do NOT abort read_handle — letting it run its shutdown arm guarantees
        // the in-flight dispatch cleanup + the one idle emission happen exactly once.
        let _ = active.shutdown_tx.send(());
        active.write_handle.abort();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex as StdMutex;

    use crate::cost_tracker::{BudgetConfig, NANODOLLARS_PER_USD};
    use crate::realtime_types::{DispatchResult, NoopDispatcher};
    use crate::storage::adapter::{ConversationEvent, Note, RecorderError};

    // ---- RealtimeAuth redaction ----------------------------------------------

    #[test]
    fn realtime_auth_debug_is_redacted() {
        let auth = RealtimeAuth::Byok(SecretString::new("sk-supersecret".into()));
        let dbg = format!("{auth:?}");
        assert!(!dbg.contains("supersecret"), "Debug must not leak the key");
        assert_eq!(dbg, "RealtimeAuth::Byok(***)");
    }

    #[test]
    fn bearer_header_carries_the_key_but_auth_does_not_serialize() {
        let auth = RealtimeAuth::Byok(SecretString::new("sk-abc".into()));
        let h = auth.bearer_header().expect("header");
        assert_eq!(h.to_str().unwrap(), "Bearer sk-abc");
        // (compile-time) RealtimeAuth has no Serialize/Clone derive — see type def.
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

    // ---- session.update ------------------------------------------------------

    #[test]
    fn session_update_includes_tools() {
        let tools = vec![ToolSchema {
            kind: "function".into(),
            name: "write_note".into(),
            description: "save a note".into(),
            parameters: serde_json::json!({ "type": "object" }),
        }];
        let v = build_session_update(&tools);
        assert_eq!(v["type"], "session.update");
        assert_eq!(v["session"]["tools"][0]["name"], "write_note");
        assert_eq!(v["session"]["tool_choice"], "auto");
    }

    // ---- usage parse ---------------------------------------------------------

    #[test]
    fn parse_usage_extracts_token_details() {
        let event = serde_json::json!({
            "type": "response.done",
            "response": { "usage": {
                "input_token_details": { "audio_tokens": 100, "text_tokens": 10, "cached_tokens": 5 },
                "output_token_details": { "audio_tokens": 200, "text_tokens": 20 }
            }}
        });
        let u = parse_usage(&event).expect("usage");
        assert_eq!(u.audio_input_tokens, 100);
        assert_eq!(u.text_input_tokens, 10);
        assert_eq!(u.cached_input_tokens, 5);
        assert_eq!(u.audio_output_tokens, 200);
        assert_eq!(u.text_output_tokens, 20);
    }

    #[test]
    fn parse_usage_missing_usage_is_none() {
        assert!(parse_usage(&serde_json::json!({ "type": "response.done" })).is_none());
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

    /// Minimal recorder double; save_cost_snapshot succeeds, the rest is unused.
    struct OkRecorder;
    impl RecorderAdapter for OkRecorder {
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
            write_tx,
            cost,
            Arc::new(OkRecorder) as Arc<dyn RecorderAdapter>,
            disp.clone() as Arc<dyn DispatcherSeam>,
            sd_rx,
            emit,
            Arc::new(TokioMutex::new(None)),
        )
        .await;

        assert_eq!(disp.calls.lock().unwrap().as_slice(), ["write_note"]);
        // The dispatch task sends two frames (item.create + response.create).
        let f1 = write_rx.recv().await.expect("item.create frame");
        let f2 = write_rx.recv().await.expect("response.create frame");
        assert!(matches!(f1, Message::Text(_)));
        assert!(matches!(f2, Message::Text(_)));
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
            write_tx,
            cost.clone(),
            Arc::new(OkRecorder) as Arc<dyn RecorderAdapter>,
            disp.clone() as Arc<dyn DispatcherSeam>,
            sd_rx,
            emit,
            Arc::new(TokioMutex::new(None)),
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
            write_tx,
            cost,
            Arc::new(OkRecorder) as Arc<dyn RecorderAdapter>,
            Arc::new(NoopDispatcher) as Arc<dyn DispatcherSeam>,
            sd_rx,
            emit,
            Arc::new(TokioMutex::new(None)),
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
            unimplemented!()
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
            write_tx,
            cost,
            Arc::new(OkRecorder) as Arc<dyn RecorderAdapter>,
            Arc::new(NoopDispatcher) as Arc<dyn DispatcherSeam>,
            sd_rx,
            emit,
            Arc::new(TokioMutex::new(None)),
        )
        .await;
        let events = log.lock().unwrap();
        assert!(events.iter().any(|(s, e)| s == "error" && e.as_deref() == Some("connection error")));
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
            write_tx,
            cost,
            Arc::new(OkRecorder) as Arc<dyn RecorderAdapter>,
            disp.clone() as Arc<dyn DispatcherSeam>,
            sd_rx,
            emit,
            Arc::new(TokioMutex::new(None)),
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
            write_tx,
            cost,
            Arc::new(FailingRecorder) as Arc<dyn RecorderAdapter>,
            Arc::new(NoopDispatcher) as Arc<dyn DispatcherSeam>,
            sd_rx,
            emit,
            Arc::new(TokioMutex::new(None)),
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
            write_tx,
            cost,
            Arc::new(OkRecorder) as Arc<dyn RecorderAdapter>,
            disp.clone() as Arc<dyn DispatcherSeam>,
            sd_rx,
            emit,
            Arc::new(TokioMutex::new(None)),
        )
        .await;
        // The unparseable frame was skipped and the following valid call dispatched.
        assert_eq!(disp.calls.lock().unwrap().as_slice(), ["write_note"]);
    }
}
