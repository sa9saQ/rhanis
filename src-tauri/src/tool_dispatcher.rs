//! Tool dispatcher (koe-2gy): routes one Realtime `function_call` to a tool.
//!
//! Flow per call (`dispatch_impl`):
//! 1. `classify(tool)` → risk tier.
//! 2. Emit a redacted `tool-event` phase=start (CAUTION rides a non-blocking
//!    `detail` note here — it never waits for approval, per `koe-caution-tier`).
//! 3. For `run_command`, enforce the shell DENY_LIST (token-level) BEFORE anything.
//! 4. Bound the incoming args size (external, attacker-influenced input).
//! 5. Route by tier: SAFE/CAUTION run immediately; DANGER → human gate, a decline
//!    returns `"user declined"` as the tool output.
//! 6. Run the registered tool (unregistered → a safe "not yet implemented" stub,
//!    so koe-s7i can plug real tools later without the dispatcher being dead).
//! 7. Emit phase=done|error and return the `conversation.item.create` +
//!    `response.create` frames for `session_manager` (koe-e3m) to send.
//!
//! **AppHandle is abstracted behind [`DispatchIo`]** (emit + approval request) so
//! the whole flow is unit-testable in WSL without a live Tauri handle or socket.
//! Production wires [`AppDispatchIo`] (real `AppHandle` + `ApprovalGate`).
//!
//! Redaction: `displaySummary`/`detail` are tool-name-derived fixed strings —
//! the args and tool output never appear there (no key/path/PII). Tool output is
//! hard-capped ([`MAX_TOOL_OUTPUT_LEN`]) as defense-in-depth on top of each
//! tool's own redaction.
//!
//! transaction N/A · idempotency_key N/A (in-process tool routing, not billing).

// The dispatcher's production path (`RealToolDispatcher::dispatch` and its
// helpers) has no in-crate caller until session_manager (koe-e3m) wires its read
// loop to it; the entire flow is exercised by this module's tests. Allow
// dead_code module-wide until koe-e3m lands, then drop this so any genuine dead
// code resurfaces.
#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use serde_json::Value;

use crate::approval_gate::{classify, ApprovalGate, ApprovalOutcome, ApprovalRisk};
use crate::events::SequenceCounter;
use crate::realtime_types::{
    function_call_output, BoxFuture, DispatchResult, DispatcherSeam, FunctionCall, ToolSchema,
};

/// Hard cap on incoming function-call args (bytes of serialized JSON). The args
/// come from the model and are attacker-influenceable; an oversized blob is a
/// DoS / cost vector, so reject it before running anything.
const MAX_ARGS_LEN: usize = 64 * 1024;

/// Hard cap on a tool's output string (bytes) before it goes back to the model.
/// Defense-in-depth: each tool also bounds its own content, but this guarantees
/// one place caps it so an oversized output cannot bloat the next request / cost.
const MAX_TOOL_OUTPUT_LEN: usize = 16 * 1024;

/// Hard cap on the tool name length. The name is model-controlled and flows into
/// `ToolEvent.tool`; bound it like the args/output caps.
const MAX_TOOL_NAME_LEN: usize = 256;

/// Shell tokens that are never allowed in a `run_command` invocation (blocked
/// outright, before the human gate). Matched at the **token** level (not
/// substring) against each whitespace/metachar-split token's basename,
/// case-insensitively, so `format` blocks the `format` command but not
/// `format_report.sh`. Scope is shells / script-hosts / destructive utilities;
/// general-purpose interpreters (python / node / ruby / …) are intentionally NOT
/// listed — they have legitimate uses (`node build.js`) and are covered by the
/// DANGER human approval gate instead of an outright block.
const DENY_TOKENS: &[&str] = &[
    "rm", "del", "erase", "format", "mkfs", "fdisk", "diskpart", "curl", "wget", "powershell",
    "pwsh", "bash", "sh", "zsh", "fish", "cmd", "reg", "rundll32", "certutil", "bitsadmin",
    "mshta", "wscript", "cscript",
];

// ---- ToolEvent (frontend contract: src/features/activity/types.ts) ----------

/// Live activity event emitted on the `tool-event` channel. Field names are
/// camelCased to match `ToolEvent` in `src/features/activity/types.ts`.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ToolEvent {
    pub event_id: String,
    /// Realtime `call_id` — groups start/…/done|error for one tool call.
    pub action_id: String,
    pub sequence: u64,
    pub tool: String,
    pub phase: String,
    pub timestamp: i64,
    pub display_summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<f64>,
}

// ---- Tool registry (the seam koe-s7i plugs into) -----------------------------

/// A type-erased async tool: receives the raw args JSON, returns
/// `Ok(output_string)` or `Err(error_string)`. The output string is the tool's
/// own responsibility to redact/size-bound; the dispatcher additionally caps it.
pub type ToolFn =
    Arc<dyn Fn(Value) -> BoxFuture<'static, Result<String, String>> + Send + Sync>;

struct RegisteredTool {
    func: ToolFn,
    schema: ToolSchema,
}

/// Maps tool name → (impl, schema). koe-s7i calls [`ToolRegistry::register`] for
/// each real tool during `lib.rs` setup; the schema travels with the impl so
/// `tool_schemas()` (sent in `session.update`) has a single source of truth.
#[derive(Default)]
pub struct ToolRegistry {
    tools: HashMap<String, RegisteredTool>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a tool impl + its `session.update` schema under `name`.
    pub fn register(&mut self, name: impl Into<String>, func: ToolFn, schema: ToolSchema) {
        let name = name.into();
        self.tools.insert(name, RegisteredTool { func, schema });
    }

    fn get(&self, name: &str) -> Option<&RegisteredTool> {
        self.tools.get(name)
    }

    /// Schemas of every registered tool (order is unspecified). For `session.update`.
    fn schemas(&self) -> Vec<ToolSchema> {
        self.tools.values().map(|t| t.schema.clone()).collect()
    }
}

// ---- DispatchIo: the AppHandle-dependent side, abstracted for tests ----------

/// The two side effects that need the live Tauri `AppHandle`: emitting a
/// `tool-event` and requesting a human approval. Abstracted so `dispatch_impl`
/// is fully unit-testable with a mock (no `AppHandle`, no socket).
pub trait DispatchIo: Send + Sync {
    fn emit_tool_event(&self, event: ToolEvent);
    /// DANGER-tier gate. Returns the fail-closed outcome (timeout/deny/drop →
    /// `Declined`). `summary` is already redacted.
    fn request_approval(
        &self,
        tool: String,
        risk: ApprovalRisk,
        summary: String,
    ) -> BoxFuture<'static, ApprovalOutcome>;
}

/// Production [`DispatchIo`]: emits via the real `AppHandle` and gates via the
/// shared `ApprovalGate`. Holds the SAME `Arc<ApprovalGate>` that `lib.rs` gives
/// `ManagedApprovalGate`, so a `resolve_tool_approval` command reaches the exact
/// pending request this dispatcher is awaiting.
pub struct AppDispatchIo {
    app: tauri::AppHandle,
    gate: Arc<ApprovalGate>,
}

impl AppDispatchIo {
    pub fn new(app: tauri::AppHandle, gate: Arc<ApprovalGate>) -> Self {
        Self { app, gate }
    }
}

impl DispatchIo for AppDispatchIo {
    fn emit_tool_event(&self, event: ToolEvent) {
        use tauri::Emitter;
        // Best-effort: a failed emit just means the UI misses this event; it
        // never changes whether the tool ran. No key/path/PII is in `event`.
        let _ = self.app.emit("tool-event", event);
    }

    fn request_approval(
        &self,
        tool: String,
        risk: ApprovalRisk,
        summary: String,
    ) -> BoxFuture<'static, ApprovalOutcome> {
        let gate = Arc::clone(&self.gate);
        let app = self.app.clone();
        Box::pin(async move { gate.request_approval(&app, &tool, risk, summary).await })
    }
}

// ---- RealToolDispatcher ------------------------------------------------------

/// The real dispatcher wired in production. Implements [`DispatcherSeam`] so
/// `session_manager` (koe-e3m) calls it through `ManagedDispatcher` with no
/// import cycle.
pub struct RealToolDispatcher {
    io: Arc<dyn DispatchIo>,
    seq: Arc<SequenceCounter>,
    registry: Arc<ToolRegistry>,
}

impl RealToolDispatcher {
    pub fn new(io: Arc<dyn DispatchIo>, seq: Arc<SequenceCounter>, registry: Arc<ToolRegistry>) -> Self {
        Self { io, seq, registry }
    }
}

impl DispatcherSeam for RealToolDispatcher {
    fn dispatch(&self, call: FunctionCall) -> BoxFuture<'static, DispatchResult> {
        // Clone owned state out of &self BEFORE the async move (the future is
        // 'static and must not borrow self).
        let io = Arc::clone(&self.io);
        let seq = Arc::clone(&self.seq);
        let registry = Arc::clone(&self.registry);
        Box::pin(async move { dispatch_impl(io, seq, registry, call).await })
    }

    fn tool_schemas(&self) -> Vec<ToolSchema> {
        self.registry.schemas()
    }
}

/// The AppHandle-free core, so tests drive it with a mock `DispatchIo`.
async fn dispatch_impl(
    io: Arc<dyn DispatchIo>,
    seq: Arc<SequenceCounter>,
    registry: Arc<ToolRegistry>,
    call: FunctionCall,
) -> DispatchResult {
    let FunctionCall { call_id, name, args } = call;
    // The name is model-controlled and flows into ToolEvent.tool; bound it before
    // emitting anything (consistent with the args/output caps).
    if name.len() > MAX_TOOL_NAME_LEN {
        return function_call_output(&call_id, error_output("tool name too long"));
    }
    let risk = classify(&name);

    // (2) phase=start. CAUTION rides a non-blocking note here; it never waits.
    let start_detail = match risk {
        ApprovalRisk::Caution => Some("caution: notified, running without approval".to_string()),
        _ => None,
    };
    io.emit_tool_event(make_event(&seq, &name, &call_id, "start", start_summary(&name), start_detail));

    // (3) run_command shell DENY_LIST — before the gate, before execution.
    if name == "run_command" && command_is_denied(&args) {
        io.emit_tool_event(make_event(
            &seq, &name, &call_id, "error",
            start_summary(&name), Some("blocked by security policy".to_string()),
        ));
        return function_call_output(&call_id, error_output("command blocked by security policy"));
    }

    // (4) bound external args.
    if args_too_large(&args) {
        io.emit_tool_event(make_event(
            &seq, &name, &call_id, "error",
            start_summary(&name), Some("arguments too large".to_string()),
        ));
        return function_call_output(&call_id, error_output("arguments too large"));
    }

    // (5) DANGER → human gate (fail-closed). SAFE/CAUTION fall through to run.
    // The gate decision derives from the canonical predicate so the policy lives
    // in one place (`ApprovalRisk::requires_approval`) and cannot drift if a new
    // tier is added.
    if risk.requires_approval() {
        let outcome = io.request_approval(name.clone(), risk, start_summary(&name)).await;
        if outcome == ApprovalOutcome::Declined {
            io.emit_tool_event(make_event(
                &seq, &name, &call_id, "error",
                start_summary(&name), Some("declined by operator".to_string()),
            ));
            return function_call_output(&call_id, error_output("user declined"));
        }
    }

    // (6) run the tool. Unregistered → safe stub (koe-s7i fills these in).
    let result = match registry.get(&name) {
        Some(t) => (t.func)(args).await,
        None => Ok("{\"status\":\"tool not yet implemented\"}".to_string()),
    };

    // (7) phase=done|error + frames.
    match result {
        Ok(output) => {
            let output = cap_output(output);
            io.emit_tool_event(make_event(&seq, &name, &call_id, "done", start_summary(&name), None));
            function_call_output(&call_id, output)
        }
        Err(_err) => {
            // The tool's raw error is NOT forwarded verbatim (it could carry a
            // path/PII); a fixed message goes to both the UI and the model.
            io.emit_tool_event(make_event(
                &seq, &name, &call_id, "error",
                start_summary(&name), Some("tool failed".to_string()),
            ));
            function_call_output(&call_id, error_output("tool execution failed"))
        }
    }
}

// ---- helpers -----------------------------------------------------------------

fn make_event(
    seq: &SequenceCounter,
    tool: &str,
    call_id: &str,
    phase: &str,
    display_summary: String,
    detail: Option<String>,
) -> ToolEvent {
    ToolEvent {
        event_id: gen_event_id(),
        action_id: call_id.to_string(),
        sequence: seq.next(),
        tool: tool.to_string(),
        phase: phase.to_string(),
        timestamp: now_ms(),
        display_summary,
        detail,
        progress: None,
    }
}

/// Redacted, fixed summary for the UI — derived only from the (trusted) tool
/// name. Never includes args, paths, keys, or output.
fn start_summary(tool: &str) -> String {
    format!("run {tool}")
}

/// Wraps a fixed error phrase as the JSON string `output` the Realtime API wants.
fn error_output(msg: &str) -> String {
    serde_json::json!({ "error": msg }).to_string()
}

/// Caps `output` to within [`MAX_TOOL_OUTPUT_LEN`] bytes. Rather than truncate
/// mid-string (which would hand the model malformed JSON), it returns a
/// well-formed JSON envelope noting the cut, with a bounded char-boundary prefix.
fn cap_output(output: String) -> String {
    if output.len() <= MAX_TOOL_OUTPUT_LEN {
        return output;
    }
    let mut end = MAX_TOOL_OUTPUT_LEN / 2;
    while !output.is_char_boundary(end) {
        end -= 1;
    }
    serde_json::json!({
        "error": "output truncated",
        "partial": &output[..end],
    })
    .to_string()
}

fn args_too_large(args: &Value) -> bool {
    serde_json::to_string(args).map(|s| s.len()).unwrap_or(usize::MAX) > MAX_ARGS_LEN
}

/// Token-level shell DENY check for `run_command`. Splits the command on
/// whitespace and shell metacharacters, takes each token's basename, strips all
/// extensions, lowercases it, and rejects if it is in [`DENY_TOKENS`]. Also
/// rejects any PowerShell encoded-command flag anywhere in the string.
///
/// TODO(koe-s7i): when `run_command` is actually registered, add an ALLOW_LIST
/// (`command_is_allowed`) as a second gate AFTER this DENY check and the human
/// approval, per CLAUDE.md. Today `run_command` is unregistered (stub), so the
/// DANGER human gate is the only active control and this DENY list is the
/// belt-and-suspenders pre-gate.
fn command_is_denied(args: &Value) -> bool {
    let cmd = args.get("command").and_then(Value::as_str).unwrap_or("");
    let low = cmd.to_ascii_lowercase();
    if low.contains("-enc") || low.contains("-encodedcommand") {
        return true;
    }
    cmd.split(|c: char| c.is_whitespace() || "|&;<>()$`\"'".contains(c))
        .filter(|t| !t.is_empty())
        .any(|tok| {
            let base = tok.rsplit(['/', '\\']).next().unwrap_or(tok);
            // Strip ALL extensions (rm.exe.bat → rm) so a multi-extension name
            // cannot slip a denied command past this pre-gate check.
            let stem = base.split('.').next().unwrap_or(base).to_ascii_lowercase();
            DENY_TOKENS.contains(&stem.as_str())
                || DENY_TOKENS.contains(&base.to_ascii_lowercase().as_str())
        })
}

/// Unguessable per-emit event id (`evt-` + 128-bit hex). A CSPRNG failure panics
/// this dispatch future; session_manager (koe-e3m) MUST run each dispatch in its
/// own `tokio::spawn` task so the panic stays contained to that one call rather
/// than tearing down the read loop.
fn gen_event_id() -> String {
    let mut bytes = [0u8; 16];
    getrandom::getrandom(&mut bytes).expect("CSPRNG must be available");
    let mut id = String::from("evt-");
    for b in bytes {
        id.push_str(&format!("{b:02x}"));
    }
    id
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Records emitted events and returns a fixed approval outcome.
    struct MockIo {
        events: Mutex<Vec<ToolEvent>>,
        approval: ApprovalOutcome,
    }
    impl MockIo {
        fn new(approval: ApprovalOutcome) -> Arc<Self> {
            Arc::new(Self { events: Mutex::new(Vec::new()), approval })
        }
        fn phases(&self) -> Vec<String> {
            self.events.lock().unwrap().iter().map(|e| e.phase.clone()).collect()
        }
        fn summaries(&self) -> Vec<String> {
            self.events.lock().unwrap().iter().map(|e| e.display_summary.clone()).collect()
        }
    }
    impl DispatchIo for MockIo {
        fn emit_tool_event(&self, event: ToolEvent) {
            self.events.lock().unwrap().push(event);
        }
        fn request_approval(
            &self,
            _tool: String,
            _risk: ApprovalRisk,
            _summary: String,
        ) -> BoxFuture<'static, ApprovalOutcome> {
            let outcome = self.approval;
            Box::pin(async move { outcome })
        }
    }

    fn echo_registry() -> Arc<ToolRegistry> {
        let mut r = ToolRegistry::new();
        r.register(
            "write_note",
            Arc::new(|args: Value| {
                Box::pin(async move { Ok(serde_json::json!({ "saved": args }).to_string()) })
            }),
            ToolSchema {
                name: "write_note".into(),
                description: "save a note".into(),
                parameters: serde_json::json!({ "type": "object" }),
            },
        );
        Arc::new(r)
    }

    fn call(name: &str, args: Value) -> FunctionCall {
        FunctionCall { call_id: format!("call_{name}"), name: name.into(), args }
    }

    async fn run(io: &Arc<MockIo>, registry: Arc<ToolRegistry>, c: FunctionCall) -> DispatchResult {
        let seq = Arc::new(SequenceCounter::new());
        dispatch_impl(io.clone() as Arc<dyn DispatchIo>, seq, registry, c).await
    }

    #[tokio::test]
    async fn safe_tool_runs_and_emits_start_then_done() {
        let io = MockIo::new(ApprovalOutcome::Approved);
        let res = run(&io, echo_registry(), call("write_note", serde_json::json!({"text": "hi"}))).await;
        assert_eq!(io.phases(), vec!["start", "done"]);
        // result echoes back the saved args under the right call_id.
        let item = &res.conversation_item_create["item"];
        assert_eq!(item["call_id"], "call_write_note");
        let out = item["output"].as_str().unwrap();
        assert!(out.contains("saved"));
        assert_eq!(res.response_create["type"], "response.create");
    }

    #[tokio::test]
    async fn unregistered_tool_returns_safe_stub_not_panic() {
        let io = MockIo::new(ApprovalOutcome::Approved);
        let res = run(&io, Arc::new(ToolRegistry::new()), call("write_note", serde_json::json!({}))).await;
        assert_eq!(io.phases(), vec!["start", "done"]);
        let out = res.conversation_item_create["item"]["output"].as_str().unwrap();
        assert!(out.contains("not yet implemented"));
    }

    #[tokio::test]
    async fn caution_tool_emits_caution_note_and_runs_without_gate() {
        // write_file is CAUTION; unregistered here, so it stubs — but the point
        // is it RUNS (reaches done) without an approval gate, with a caution note.
        let io = MockIo::new(ApprovalOutcome::Declined); // would block if gated
        let res = run(&io, Arc::new(ToolRegistry::new()), call("write_file", serde_json::json!({}))).await;
        assert_eq!(io.phases(), vec!["start", "done"], "CAUTION must not gate");
        let start = &io.events.lock().unwrap()[0];
        assert_eq!(start.detail.as_deref(), Some("caution: notified, running without approval"));
        let out = res.conversation_item_create["item"]["output"].as_str().unwrap();
        assert!(out.contains("not yet implemented"));
    }

    #[tokio::test]
    async fn danger_tool_declined_returns_user_declined() {
        let io = MockIo::new(ApprovalOutcome::Declined);
        let res = run(&io, Arc::new(ToolRegistry::new()), call("delete_file", serde_json::json!({"path": "x"}))).await;
        // start, then error (declined) — never reaches done.
        assert_eq!(io.phases(), vec!["start", "error"]);
        let out = res.conversation_item_create["item"]["output"].as_str().unwrap();
        assert!(out.contains("user declined"));
    }

    #[tokio::test]
    async fn danger_tool_approved_runs() {
        let io = MockIo::new(ApprovalOutcome::Approved);
        let res = run(&io, Arc::new(ToolRegistry::new()), call("delete_file", serde_json::json!({"path": "x"}))).await;
        assert_eq!(io.phases(), vec!["start", "done"]);
        let out = res.conversation_item_create["item"]["output"].as_str().unwrap();
        assert!(out.contains("not yet implemented"));
    }

    #[tokio::test]
    async fn run_command_denylist_blocks_before_gate() {
        // Even with approval granted, a deny-listed command never runs.
        let io = MockIo::new(ApprovalOutcome::Approved);
        let res = run(&io, Arc::new(ToolRegistry::new()), call("run_command", serde_json::json!({"command": "rm -rf /"}))).await;
        assert_eq!(io.phases(), vec!["start", "error"]);
        let out = res.conversation_item_create["item"]["output"].as_str().unwrap();
        assert!(out.contains("security policy"));
    }

    #[test]
    fn denylist_is_token_level_not_substring() {
        // basename `format` is blocked …
        assert!(command_is_denied(&serde_json::json!({"command": "format C:"})));
        assert!(command_is_denied(&serde_json::json!({"command": "/usr/bin/curl http://x"})));
        assert!(command_is_denied(&serde_json::json!({"command": "powershell -enc ABC"})));
        assert!(command_is_denied(&serde_json::json!({"command": "echo hi | rm x"})));
        assert!(command_is_denied(&serde_json::json!({"command": "RM file"}))); // case-insensitive
        assert!(command_is_denied(&serde_json::json!({"command": "rm.exe.bat -rf /"}))); // multi-extension
        // … but a command merely CONTAINING the substring is not.
        assert!(!command_is_denied(&serde_json::json!({"command": "format_report.sh"})));
        assert!(!command_is_denied(&serde_json::json!({"command": "node build.js"})));
    }

    #[tokio::test]
    async fn oversized_args_are_rejected() {
        let io = MockIo::new(ApprovalOutcome::Approved);
        let big = "x".repeat(MAX_ARGS_LEN + 10);
        let res = run(&io, echo_registry(), call("write_note", serde_json::json!({"text": big}))).await;
        assert_eq!(io.phases(), vec!["start", "error"]);
        let out = res.conversation_item_create["item"]["output"].as_str().unwrap();
        assert!(out.contains("too large"));
    }

    #[tokio::test]
    async fn display_summary_never_leaks_args() {
        let io = MockIo::new(ApprovalOutcome::Approved);
        let secret = "/home/user/.ssh/id_rsa";
        let _ = run(&io, echo_registry(), call("write_note", serde_json::json!({"text": secret}))).await;
        for s in io.summaries() {
            assert!(!s.contains(secret), "summary must not echo args: {s}");
            assert!(!s.contains("id_rsa"));
        }
    }

    #[tokio::test]
    async fn event_ids_unique_and_sequence_monotonic() {
        let io = MockIo::new(ApprovalOutcome::Approved);
        let _ = run(&io, echo_registry(), call("write_note", serde_json::json!({}))).await;
        let events = io.events.lock().unwrap();
        assert_eq!(events.len(), 2);
        assert_ne!(events[0].event_id, events[1].event_id);
        assert!(events[0].event_id.starts_with("evt-"));
        assert!(events[1].sequence > events[0].sequence);
    }

    #[test]
    fn registry_schemas_round_trip() {
        let r = echo_registry();
        let schemas = r.schemas();
        assert_eq!(schemas.len(), 1);
        assert_eq!(schemas[0].name, "write_note");
    }

    #[test]
    fn tool_event_serializes_to_camelcase() {
        let e = ToolEvent {
            event_id: "evt-1".into(),
            action_id: "call_1".into(),
            sequence: 3,
            tool: "write_note".into(),
            phase: "start".into(),
            timestamp: 100,
            display_summary: "run write_note".into(),
            detail: None,
            progress: None,
        };
        let v = serde_json::to_value(&e).unwrap();
        assert_eq!(v["eventId"], "evt-1");
        assert_eq!(v["actionId"], "call_1");
        assert_eq!(v["displaySummary"], "run write_note");
        assert!(v.get("event_id").is_none());
        // optional fields omitted when None
        assert!(v.get("detail").is_none());
        assert!(v.get("progress").is_none());
    }

    #[test]
    fn cap_output_wraps_oversized_in_truncation_envelope() {
        let big = "あ".repeat(MAX_TOOL_OUTPUT_LEN); // 3 bytes each → over the cap
        let capped = cap_output(big);
        assert!(capped.len() <= MAX_TOOL_OUTPUT_LEN);
        // Well-formed JSON envelope, not a truncated raw string the model can't parse.
        let v: serde_json::Value = serde_json::from_str(&capped).expect("valid JSON envelope");
        assert_eq!(v["error"], "output truncated");
        assert!(v["partial"].is_string());
        // Under-cap output passes through unchanged.
        assert_eq!(cap_output("small".into()), "small");
    }

    #[tokio::test]
    async fn registered_tool_error_is_redacted_not_forwarded() {
        // A tool whose Err carries a path must NOT have that path forwarded to
        // the model or surfaced in any emitted event.
        let mut r = ToolRegistry::new();
        r.register(
            "write_note",
            Arc::new(|_args: Value| {
                Box::pin(async move { Err("/home/user/.ssh/id_rsa leaked".to_string()) })
            }),
            ToolSchema {
                name: "write_note".into(),
                description: "x".into(),
                parameters: serde_json::json!({}),
            },
        );
        let io = MockIo::new(ApprovalOutcome::Approved);
        let res = run(&io, Arc::new(r), call("write_note", serde_json::json!({}))).await;
        assert_eq!(io.phases(), vec!["start", "error"]);
        let out = res.conversation_item_create["item"]["output"].as_str().unwrap();
        assert!(!out.contains("id_rsa"), "raw tool error must not reach the model");
        assert!(!out.contains(".ssh"));
        assert!(out.contains("tool execution failed"));
        for s in io.summaries() {
            assert!(!s.contains("id_rsa"), "raw error must not leak into events");
        }
    }

    #[tokio::test]
    async fn oversized_tool_name_rejected_without_emitting() {
        let io = MockIo::new(ApprovalOutcome::Approved);
        let long = "x".repeat(MAX_TOOL_NAME_LEN + 1);
        let res = run(&io, Arc::new(ToolRegistry::new()), call(&long, serde_json::json!({}))).await;
        // Rejected before any event is emitted — the long name never reaches ToolEvent.
        assert!(io.phases().is_empty());
        let out = res.conversation_item_create["item"]["output"].as_str().unwrap();
        assert!(out.contains("too long"));
    }
}
