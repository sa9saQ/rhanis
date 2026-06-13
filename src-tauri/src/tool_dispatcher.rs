//! Tool dispatcher (rhanis-2gy): routes one Realtime `function_call` to a tool.
//!
//! Flow per call (`dispatch_impl`):
//! 1. `classify(tool)` → risk tier.
//! 2. Emit a redacted `tool-event` phase=start (CAUTION rides a non-blocking
//!    `detail` note here — it never waits for approval, per `rhanis-caution-tier`).
//! 3. For `run_command`, enforce the shell DENY_LIST (token-level) BEFORE anything.
//! 4. Bound the incoming args size (external, attacker-influenced input).
//!    Then reject an UNREGISTERED tool with phase=error BEFORE the gate
//!    (rhanis-r2o — the old Ok-stub showed phase=done for a no-op, and the
//!    operator must never be interrupted to approve something that cannot run).
//! 5. Route by tier: SAFE/CAUTION run immediately; DANGER → human gate, a decline
//!    returns `"user declined"` as the tool output.
//! 6. Run the registered tool (rhanis-s7i plugs real tools in).
//! 7. Emit phase=done|error and return the `conversation.item.create` +
//!    `response.create` frames for `session_manager` (rhanis-e3m) to send.
//!
//! **AppHandle is abstracted behind [`DispatchIo`]** (emit + approval request) so
//! the whole flow is unit-testable in WSL without a live Tauri handle or socket.
//! Production wires [`AppDispatchIo`] (real `AppHandle` + `ApprovalGate`).
//!
//! Redaction: `displaySummary` is `run {tool}` plus a SAFE target descriptor
//! (home-relative path / first command token / URL host — rhanis-whf, see
//! [`crate::display_descriptor`]); raw args and tool output never appear there
//! (no key / full absolute path / PII). Tool output is hard-capped
//! ([`MAX_TOOL_OUTPUT_LEN`]) as defense-in-depth on top of each tool's own
//! redaction.
//!
//! transaction N/A · idempotency_key N/A (in-process tool routing, not billing).

// The dispatcher's production path (`RealToolDispatcher::dispatch` and its
// helpers) has no in-crate caller until session_manager (rhanis-e3m) wires its read
// loop to it; the entire flow is exercised by this module's tests. Allow
// dead_code module-wide until rhanis-e3m lands, then drop this so any genuine dead
// code resurfaces.
#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use serde_json::Value;

use crate::approval_gate::{classify, ApprovalGate, ApprovalOutcome, ApprovalRisk};
use crate::display_descriptor::run_summary;
use crate::events::SequenceCounter;
use crate::permission_policy::{decide, PolicyDecision, PolicyProvider};
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
/// `ToolEvent.tool`; bound it like the args/output caps. `pub(crate)` so the
/// session_manager conversation journal (rhanis-emd) bounds the same model-controlled
/// field with the same limit instead of re-introducing a magic number.
pub(crate) const MAX_TOOL_NAME_LEN: usize = 256;

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

// ---- Tool registry (the seam rhanis-s7i plugs into) -----------------------------

/// A type-erased async tool: receives the raw args JSON, returns
/// `Ok(output_string)` or `Err(error_string)`. The output string is the tool's
/// own responsibility to redact/size-bound; the dispatcher additionally caps it.
pub type ToolFn =
    Arc<dyn Fn(Value) -> BoxFuture<'static, Result<String, String>> + Send + Sync>;

struct RegisteredTool {
    func: ToolFn,
    schema: ToolSchema,
}

/// Maps tool name → (impl, schema). rhanis-s7i calls [`ToolRegistry::register`] for
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
    /// Public so `tools/mod.rs` tests can inspect the registry directly.
    pub fn tool_schemas(&self) -> Vec<ToolSchema> {
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
/// `session_manager` (rhanis-e3m) calls it through `ManagedDispatcher` with no
/// import cycle.
pub struct RealToolDispatcher {
    io: Arc<dyn DispatchIo>,
    seq: Arc<SequenceCounter>,
    registry: Arc<ToolRegistry>,
    /// User permission policy seam (rhanis-351). Read per-dispatch so a settings
    /// edit takes effect immediately; a load failure fails closed (see
    /// `SettingsPolicyProvider` / `PolicyState::Unavailable`).
    policy: Arc<dyn PolicyProvider>,
}

impl RealToolDispatcher {
    pub fn new(
        io: Arc<dyn DispatchIo>,
        seq: Arc<SequenceCounter>,
        registry: Arc<ToolRegistry>,
        policy: Arc<dyn PolicyProvider>,
    ) -> Self {
        Self {
            io,
            seq,
            registry,
            policy,
        }
    }
}

impl DispatcherSeam for RealToolDispatcher {
    fn dispatch(&self, call: FunctionCall) -> BoxFuture<'static, DispatchResult> {
        // Clone owned state out of &self BEFORE the async move (the future is
        // 'static and must not borrow self).
        let io = Arc::clone(&self.io);
        let seq = Arc::clone(&self.seq);
        let registry = Arc::clone(&self.registry);
        let policy = Arc::clone(&self.policy);
        Box::pin(async move { dispatch_impl(io, seq, registry, policy, call).await })
    }

    fn tool_schemas(&self) -> Vec<ToolSchema> {
        self.registry.tool_schemas()
    }
}

/// The AppHandle-free core, so tests drive it with a mock `DispatchIo`.
async fn dispatch_impl(
    io: Arc<dyn DispatchIo>,
    seq: Arc<SequenceCounter>,
    registry: Arc<ToolRegistry>,
    policy: Arc<dyn PolicyProvider>,
    call: FunctionCall,
) -> DispatchResult {
    let FunctionCall { call_id, name, args } = call;
    // The name is model-controlled and flows into ToolEvent.tool; bound it before
    // emitting anything (consistent with the args/output caps).
    if name.len() > MAX_TOOL_NAME_LEN {
        return function_call_output(&call_id, error_output("tool name too long"));
    }
    let risk = classify(&name);

    // One safe, human-meaningful summary per call (rhanis-whf): the SAME string is
    // shown in every phase event AND the approval modal, and it derives from the
    // SAME parsed `args` the policy and the tool consume (no display/exec skew).
    // Computing it before the args-size check below is safe: production frames
    // are already bounded upstream (parse_frame caps args_raw), derivation is
    // one O(n) scan, and the descriptor itself is capped to ~96 chars.
    let summary = run_summary(&name, &args);

    // (2) phase=start. CAUTION rides a non-blocking note here; it never waits.
    let start_detail = match risk {
        ApprovalRisk::Caution => Some("caution: notified, running without approval".to_string()),
        _ => None,
    };
    io.emit_tool_event(make_event(&seq, &name, &call_id, "start", summary.clone(), start_detail));

    // (3) run_command shell DENY_LIST — before the gate, before execution.
    if name == "run_command" && command_is_denied(&args) {
        io.emit_tool_event(make_event(
            &seq, &name, &call_id, "error",
            summary.clone(), Some("blocked by security policy".to_string()),
        ));
        return function_call_output(&call_id, error_output("command blocked by security policy"));
    }

    // (4) bound external args.
    if args_too_large(&args) {
        io.emit_tool_event(make_event(
            &seq, &name, &call_id, "error",
            summary.clone(), Some("arguments too large".to_string()),
        ));
        return function_call_output(&call_id, error_output("arguments too large"));
    }

    // (4.5) Unregistered tool: fail VISIBLY before the human gate (rhanis-r2o).
    // The old stub returned Ok (phase=done) — an approved DANGER op that
    // silently no-ops is indistinguishable from a successful one in the
    // ActivityLog, a lie in a product whose thesis is calibrated transparency.
    // Checking BEFORE the gate also means the operator is never interrupted to
    // approve something that cannot run. After the deny-list (3) on purpose:
    // a deny-listed command reports the security block, not "not implemented"
    // (the more safety-relevant signal wins). rhanis-s7i fills the registry in.
    let Some(tool) = registry.get(&name) else {
        io.emit_tool_event(make_event(
            &seq, &name, &call_id, "error",
            summary.clone(), Some("tool not implemented".to_string()),
        ));
        return function_call_output(&call_id, error_output("tool not implemented"));
    };

    // (5) Gate decision = built-in tier COMPOSED with the user permission policy
    // (rhanis-351). The policy can only ADD safety: `AutoApprove` skips the gate for
    // an allow-listed target; `Default` keeps the tier behaviour (DANGER gates,
    // SAFE/CAUTION run); `RequireApproval` forces the gate even for a tier that
    // would otherwise run (deny-list / baseline / unresolved path / strict URL /
    // policy-unavailable). It never relaxes a DANGER op except via an explicit
    // per-folder opt-in. The shell DENY/ALLOW list + real-IO O_NOFOLLOW guards are
    // independent and still apply (defense in depth).
    let must_gate = match decide(&policy.current_policy(), &name, risk, &args) {
        PolicyDecision::AutoApprove => false,
        PolicyDecision::Default => risk.requires_approval(),
        PolicyDecision::RequireApproval => true,
    };
    if must_gate {
        // The approval-required event is always DANGER-tier: requiring confirmation
        // IS the danger UX, and the frontend `ApprovalRisk` union only carries
        // CAUTION/DANGER (never SAFE). The summary carries only the safe target
        // descriptor (home-relative path / first token / host — rhanis-whf), never
        // the raw args. A decline blocks the tool (fail-closed).
        let outcome = io
            .request_approval(name.clone(), ApprovalRisk::Danger, summary.clone())
            .await;
        if outcome == ApprovalOutcome::Declined {
            io.emit_tool_event(make_event(
                &seq, &name, &call_id, "error",
                summary.clone(), Some("declined by operator".to_string()),
            ));
            return function_call_output(&call_id, error_output("user declined"));
        }
    }

    // (5.5) run_command ALLOW_LIST: checked AFTER deny-list (step 3) AND after
    // the human gate (step 5). Only commands whose executable basename appears in
    // `tools::ALLOW_COMMANDS` may proceed. A command that passes the deny-list and
    // survives human approval but is NOT in the allow-list is blocked here.
    // CLAUDE.md: "DENY_LIST … を先に判定、その後 ALLOW_LIST ホワイトリスト".
    if name == "run_command" && !crate::tools::command_is_allowed(&args) {
        io.emit_tool_event(make_event(
            &seq, &name, &call_id, "error",
            summary.clone(), Some("command not in allow list".to_string()),
        ));
        return function_call_output(&call_id, error_output("command not permitted"));
    }

    // (6) run the tool (presence pinned at 4.5 — `tool` borrows the local Arc).
    let result = (tool.func)(args).await;

    // (7) phase=done|error + frames.
    match result {
        Ok(output) => {
            let output = cap_output(output);
            io.emit_tool_event(make_event(&seq, &name, &call_id, "done", summary.clone(), None));
            function_call_output(&call_id, output)
        }
        Err(_err) => {
            // The tool's raw error is NOT forwarded verbatim (it could carry a
            // path/PII); a fixed message goes to both the UI and the model.
            io.emit_tool_event(make_event(
                &seq, &name, &call_id, "error",
                summary.clone(), Some("tool failed".to_string()),
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
    // JSON-escaping `partial` (quotes/backslashes/control chars) can inflate it
    // up to ~6x, so shrink the prefix until the SERIALIZED envelope fits the cap.
    let mut end = MAX_TOOL_OUTPUT_LEN / 2;
    loop {
        while end > 0 && !output.is_char_boundary(end) {
            end -= 1;
        }
        let envelope = serde_json::json!({
            "error": "output truncated",
            "partial": &output[..end],
        })
        .to_string();
        if envelope.len() <= MAX_TOOL_OUTPUT_LEN || end == 0 {
            return envelope;
        }
        end /= 2;
    }
}

fn args_too_large(args: &Value) -> bool {
    serde_json::to_string(args).map(|s| s.len()).unwrap_or(usize::MAX) > MAX_ARGS_LEN
}

/// Token-level shell DENY check for `run_command`. Splits the command on
/// whitespace and shell metacharacters, takes each token's basename, strips all
/// extensions, lowercases it, and rejects if it is in [`DENY_TOKENS`]. Also
/// rejects any PowerShell encoded-command flag anywhere in the string.
///
/// DENY check is step 3 in the dispatch flow; the ALLOW_LIST (`command_is_allowed`
/// in `tools/mod.rs`) is step 5.5 — called after this AND after the 30s human
/// gate (rhanis-s7i). Per CLAUDE.md: "DENY_LIST … を先に判定、その後 ALLOW_LIST ホワイトリスト".
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
/// this dispatch future; session_manager (rhanis-e3m) MUST run each dispatch in its
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

    /// Records emitted events + approval-request summaries and returns a fixed
    /// approval outcome.
    struct MockIo {
        events: Mutex<Vec<ToolEvent>>,
        approvals: Mutex<Vec<String>>,
        approval: ApprovalOutcome,
    }
    impl MockIo {
        fn new(approval: ApprovalOutcome) -> Arc<Self> {
            Arc::new(Self {
                events: Mutex::new(Vec::new()),
                approvals: Mutex::new(Vec::new()),
                approval,
            })
        }
        fn phases(&self) -> Vec<String> {
            self.events.lock().unwrap().iter().map(|e| e.phase.clone()).collect()
        }
        fn summaries(&self) -> Vec<String> {
            self.events.lock().unwrap().iter().map(|e| e.display_summary.clone()).collect()
        }
        fn approval_summaries(&self) -> Vec<String> {
            self.approvals.lock().unwrap().clone()
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
            summary: String,
        ) -> BoxFuture<'static, ApprovalOutcome> {
            self.approvals.lock().unwrap().push(summary);
            let outcome = self.approval;
            Box::pin(async move { outcome })
        }
    }

    /// Returns a fixed [`PolicyState`] (rhanis-351). The default tests use an empty
    /// loaded policy (auto-approve nothing → existing tier behaviour preserved).
    struct MockPolicyProvider(crate::permission_policy::PolicyState);
    impl PolicyProvider for MockPolicyProvider {
        fn current_policy(&self) -> crate::permission_policy::PolicyState {
            self.0.clone()
        }
    }
    fn empty_policy() -> Arc<dyn PolicyProvider> {
        Arc::new(MockPolicyProvider(crate::permission_policy::PolicyState::Loaded(
            crate::permission_policy::PermissionPolicy::default(),
        )))
    }

    fn echo_registry() -> Arc<ToolRegistry> {
        let mut r = ToolRegistry::new();
        r.register(
            "write_note",
            Arc::new(|args: Value| {
                Box::pin(async move { Ok(serde_json::json!({ "saved": args }).to_string()) })
            }),
            ToolSchema {
                kind: "function".into(),
                name: "write_note".into(),
                description: "save a note".into(),
                parameters: serde_json::json!({ "type": "object" }),
            },
        );
        Arc::new(r)
    }

    /// Registry with a trivial Ok tool registered under `name` — for tests whose
    /// POINT is the gate / policy / descriptor flow, not the tool body (rhanis-r2o
    /// made the unregistered path a pre-gate error, so flow tests must register
    /// the tool they exercise).
    fn registry_with(name: &str) -> Arc<ToolRegistry> {
        let mut r = ToolRegistry::new();
        r.register(
            name,
            Arc::new(|_args: Value| {
                Box::pin(async move { Ok("{\"status\":\"ran\"}".to_string()) })
            }),
            ToolSchema {
                kind: "function".into(),
                name: name.into(),
                description: "test tool".into(),
                parameters: serde_json::json!({ "type": "object" }),
            },
        );
        Arc::new(r)
    }

    fn call(name: &str, args: Value) -> FunctionCall {
        FunctionCall { call_id: format!("call_{name}"), name: name.into(), args }
    }

    async fn run(io: &Arc<MockIo>, registry: Arc<ToolRegistry>, c: FunctionCall) -> DispatchResult {
        run_with_policy(io, registry, c, empty_policy()).await
    }

    async fn run_with_policy(
        io: &Arc<MockIo>,
        registry: Arc<ToolRegistry>,
        c: FunctionCall,
        policy: Arc<dyn PolicyProvider>,
    ) -> DispatchResult {
        let seq = Arc::new(SequenceCounter::new());
        dispatch_impl(io.clone() as Arc<dyn DispatchIo>, seq, registry, policy, c).await
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
    async fn unregistered_tool_fails_visibly_not_stub_ok() {
        // rhanis-r2o: the old stub returned Ok + phase=done — "executed" in the
        // ActivityLog while nothing ran, a lie in a glass-box product. An
        // unregistered tool must surface phase=error + a fixed detail, and the
        // model must receive an error frame.
        let io = MockIo::new(ApprovalOutcome::Approved);
        let res = run(&io, Arc::new(ToolRegistry::new()), call("write_note", serde_json::json!({}))).await;
        assert_eq!(io.phases(), vec!["start", "error"]);
        let events = io.events.lock().unwrap();
        assert_eq!(
            events[1].detail.as_deref(),
            Some("tool not implemented"),
            "the error row must say WHY"
        );
        drop(events);
        let out = res.conversation_item_create["item"]["output"].as_str().unwrap();
        assert!(out.contains("tool not implemented"), "model must see the error: {out}");
    }

    #[tokio::test]
    async fn unregistered_danger_tool_fails_before_the_human_gate() {
        // rhanis-r2o: the registry check runs BEFORE the approval gate — the
        // operator is never interrupted to approve something that cannot run.
        // Declined MockIo proves the gate was not consulted: if it were, the
        // output would be "user declined", not "tool not implemented".
        let io = MockIo::new(ApprovalOutcome::Declined);
        let res = run(&io, Arc::new(ToolRegistry::new()), call("delete_file", serde_json::json!({"path": "x"}))).await;
        assert_eq!(io.phases(), vec!["start", "error"]);
        let out = res.conversation_item_create["item"]["output"].as_str().unwrap();
        assert!(out.contains("tool not implemented"), "gate must not fire: {out}");
        assert!(
            io.approval_summaries().is_empty(),
            "no approval modal for a tool that cannot run"
        );
    }

    #[tokio::test]
    async fn caution_tool_emits_caution_note_and_runs_without_gate() {
        // write_file is CAUTION (registered here — rhanis-r2o): the point is it
        // RUNS (reaches done) without an approval gate, with a caution note.
        let io = MockIo::new(ApprovalOutcome::Declined); // would block if gated
        let res = run(&io, registry_with("write_file"), call("write_file", serde_json::json!({}))).await;
        assert_eq!(io.phases(), vec!["start", "done"], "CAUTION must not gate");
        let start = &io.events.lock().unwrap()[0];
        assert_eq!(start.detail.as_deref(), Some("caution: notified, running without approval"));
        let out = res.conversation_item_create["item"]["output"].as_str().unwrap();
        assert!(out.contains("ran"));
    }

    #[tokio::test]
    async fn danger_tool_declined_returns_user_declined() {
        let io = MockIo::new(ApprovalOutcome::Declined);
        let res = run(&io, registry_with("delete_file"), call("delete_file", serde_json::json!({"path": "x"}))).await;
        // start, then error (declined) — never reaches done.
        assert_eq!(io.phases(), vec!["start", "error"]);
        let out = res.conversation_item_create["item"]["output"].as_str().unwrap();
        assert!(out.contains("user declined"));
    }

    #[tokio::test]
    async fn danger_tool_approved_runs() {
        let io = MockIo::new(ApprovalOutcome::Approved);
        let res = run(&io, registry_with("delete_file"), call("delete_file", serde_json::json!({"path": "x"}))).await;
        assert_eq!(io.phases(), vec!["start", "done"]);
        let out = res.conversation_item_create["item"]["output"].as_str().unwrap();
        assert!(out.contains("ran"));
    }

    #[tokio::test]
    async fn run_command_denylist_blocks_before_gate() {
        // Even with approval granted, a deny-listed command never runs.
        // Registry intentionally EMPTY (do NOT "fix" to registry_with): with
        // rhanis-r2o this also pins deny-list (3) firing BEFORE the unregistered
        // check (4.5) — the security block must win over "tool not implemented".
        let io = MockIo::new(ApprovalOutcome::Approved);
        let res = run(&io, Arc::new(ToolRegistry::new()), call("run_command", serde_json::json!({"command": "rm -rf /"}))).await;
        assert_eq!(io.phases(), vec!["start", "error"]);
        let out = res.conversation_item_create["item"]["output"].as_str().unwrap();
        assert!(out.contains("security policy"));
        assert!(
            !out.contains("tool not implemented"),
            "deny-list must report the security block, not the registry miss"
        );
    }

    #[tokio::test]
    async fn run_command_allowlist_blocks_after_gate() {
        // `python` passes the DENY_LIST (not a deny-listed command) but is NOT in
        // ALLOW_COMMANDS. Even with human approval (Approved), the dispatcher's
        // step 5.5 ALLOW_LIST check must block it and return "not permitted".
        //
        // This mirrors `run_command_denylist_blocks_before_gate` for the ALLOW_LIST
        // path: deny-list check (step 3) passes → human gate fires and approves
        // (step 5) → ALLOW_LIST gate (step 5.5) rejects.
        let io = MockIo::new(ApprovalOutcome::Approved);
        let res = run(
            &io,
            registry_with("run_command"),
            call("run_command", serde_json::json!({"command": "python script.py"})),
        )
        .await;
        // Phases: start (step 2) → error (step 5.5 ALLOW_LIST block).
        // The human gate fires between those two phases (Approved), then the
        // ALLOW_LIST check terminates with an error.
        assert_eq!(io.phases(), vec!["start", "error"]);
        let out = res.conversation_item_create["item"]["output"].as_str().unwrap();
        assert!(
            out.contains("not permitted"),
            "ALLOW_LIST block must say 'not permitted', got: {out}"
        );
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

    // ---- rhanis-whf: safe target descriptors in the gate + the log ----

    #[tokio::test]
    async fn approval_summary_shows_home_relative_target() {
        let io = MockIo::new(ApprovalOutcome::Approved);
        let home = dirs_next::home_dir().expect("test env has a home dir");
        let p = home.join("Documents").join("report.txt");
        let _ = run(
            &io,
            registry_with("delete_file"),
            call("delete_file", serde_json::json!({ "path": p.to_string_lossy() })),
        )
        .await;
        let approvals = io.approval_summaries();
        assert_eq!(approvals.len(), 1, "DANGER must gate exactly once");
        assert!(approvals[0].contains("report.txt"), "{}", approvals[0]);
        assert!(approvals[0].contains('~'), "{}", approvals[0]);
        let home_s = home.to_string_lossy().to_string();
        assert!(
            !approvals[0].contains(&home_s),
            "modal must not echo the raw home prefix: {}",
            approvals[0]
        );
        // The phase events tell the SAME story as the modal.
        for s in io.summaries() {
            assert!(s.contains("report.txt"), "{s}");
            assert!(!s.contains(&home_s), "{s}");
        }
    }

    #[tokio::test]
    async fn approval_summary_shows_command_first_token_only() {
        let io = MockIo::new(ApprovalOutcome::Approved);
        let _ = run(
            &io,
            registry_with("run_command"),
            call("run_command", serde_json::json!({ "command": "ls -la /home/user/secret-dir" })),
        )
        .await;
        let approvals = io.approval_summaries();
        assert_eq!(approvals.len(), 1);
        assert!(approvals[0].contains("ls"), "{}", approvals[0]);
        assert!(
            !approvals[0].contains("secret-dir"),
            "argv beyond the first token must not leak: {}",
            approvals[0]
        );
    }

    #[tokio::test]
    async fn url_summary_shows_host_only() {
        // Empty policy → strict URL default → open_url is gated; Approved → runs.
        let io = MockIo::new(ApprovalOutcome::Approved);
        let _ = run(
            &io,
            registry_with("open_url"),
            call(
                "open_url",
                serde_json::json!({ "url": "https://user:tok-123@site.example/cb?key=apikey" }),
            ),
        )
        .await;
        let approvals = io.approval_summaries();
        assert_eq!(approvals.len(), 1);
        assert!(approvals[0].contains("site.example"), "{}", approvals[0]);
        for s in io.summaries().iter().chain(approvals.iter()) {
            assert!(!s.contains("apikey"), "query must not leak: {s}");
            assert!(!s.contains("tok-123"), "userinfo must not leak: {s}");
            assert!(!s.contains("/cb"), "path must not leak: {s}");
        }
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
        let schemas = r.tool_schemas();
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

    #[test]
    fn cap_output_envelope_fits_cap_even_with_heavy_escaping() {
        // All-quotes output OVER the cap: JSON-escaping nearly doubles it. The
        // serialized envelope must STILL be within the cap and parse as valid JSON.
        let quotes = "\"".repeat(MAX_TOOL_OUTPUT_LEN + 100);
        let capped = cap_output(quotes);
        assert!(capped.len() <= MAX_TOOL_OUTPUT_LEN, "escaped envelope must fit cap: {}", capped.len());
        let _: serde_json::Value = serde_json::from_str(&capped).expect("valid JSON");
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
                kind: "function".into(),
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

    // ---- permission policy composition (rhanis-351) -----------------------------
    //
    // These prove the policy layer actually changes the gate decision in the
    // dispatcher, including via the REAL settings store (the end-to-end wiring
    // evidence: settings file → SettingsPolicyProvider → dispatcher → gate).

    use crate::permission_policy::{AllowedFolder, PermissionPolicy, PolicyState};
    use crate::settings_store::{AppSettings, JsonSettingsStore, SettingsPolicyProvider, SettingsStore};

    fn loaded(policy: PermissionPolicy) -> Arc<dyn PolicyProvider> {
        Arc::new(MockPolicyProvider(PolicyState::Loaded(policy)))
    }

    #[tokio::test]
    async fn policy_unavailable_forces_gate_on_safe_read() {
        // Settings unavailable → a SAFE read with an absolute path must be gated
        // (the deny protections are NOT dropped). A decline blocks it.
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("note.txt");
        std::fs::write(&file, b"x").unwrap();
        let io = MockIo::new(ApprovalOutcome::Declined);
        let provider: Arc<dyn PolicyProvider> = Arc::new(MockPolicyProvider(PolicyState::Unavailable));
        let res = run_with_policy(
            &io,
            registry_with("read_file"),
            call("read_file", serde_json::json!({ "path": file.to_str().unwrap() })),
            provider,
        )
        .await;
        assert_eq!(io.phases(), vec!["start", "error"], "SAFE read must be force-gated when policy unavailable");
        let out = res.conversation_item_create["item"]["output"].as_str().unwrap();
        assert!(out.contains("user declined"));
    }

    #[tokio::test]
    async fn allowed_danger_folder_skips_gate_even_when_io_would_decline() {
        // A delete_file inside an allow_danger folder auto-runs: the gate is
        // skipped, so the Declining MockIo never blocks it (reaches "done").
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("doomed.txt");
        std::fs::write(&file, b"x").unwrap();
        let policy = PermissionPolicy {
            allowed_folders: vec![AllowedFolder {
                path: dir.path().canonicalize().unwrap().to_string_lossy().into_owned(),
                allow_danger: true,
            }],
            ..Default::default()
        };
        let io = MockIo::new(ApprovalOutcome::Declined); // would block if gated
        run_with_policy(
            &io,
            registry_with("delete_file"),
            call("delete_file", serde_json::json!({ "path": file.to_str().unwrap() })),
            loaded(policy),
        )
        .await;
        assert_eq!(io.phases(), vec!["start", "done"], "opt-in DANGER must auto-run (gate skipped)");
    }

    /// EVIDENCE: a denied folder configured through the REAL JsonSettingsStore
    /// forces a gate on a SAFE read — proving UI → settings_store → provider →
    /// dispatcher is wired end to end (not just the in-memory mock).
    #[tokio::test]
    async fn denied_folder_via_real_settings_store_forces_gate() {
        let data = tempfile::tempdir().unwrap();
        let path = data.path().join("koe-settings.json");
        let store: Arc<dyn SettingsStore> = Arc::new(JsonSettingsStore::new(path));

        let work = tempfile::tempdir().unwrap();
        let file = work.path().join("secret.txt");
        std::fs::write(&file, b"x").unwrap();
        let policy = PermissionPolicy {
            denied_folders: vec![work.path().canonicalize().unwrap().to_string_lossy().into_owned()],
            ..Default::default()
        };
        store
            .save(&AppSettings { permission_policy: policy, ..AppSettings::default() })
            .expect("seed settings");
        let provider: Arc<dyn PolicyProvider> = Arc::new(SettingsPolicyProvider(Arc::clone(&store)));

        let io = MockIo::new(ApprovalOutcome::Declined);
        let res = run_with_policy(
            &io,
            registry_with("read_file"),
            call("read_file", serde_json::json!({ "path": file.to_str().unwrap() })),
            provider,
        )
        .await;
        assert_eq!(io.phases(), vec!["start", "error"], "denied folder must force a gate via the real store");
        let out = res.conversation_item_create["item"]["output"].as_str().unwrap();
        assert!(out.contains("user declined"));
    }

    /// EVIDENCE: an allow_danger folder configured through the REAL settings store
    /// auto-executes a DANGER delete (gate skipped) — the relaxation half of the
    /// same wiring.
    #[tokio::test]
    async fn allow_danger_via_real_settings_store_auto_executes() {
        let data = tempfile::tempdir().unwrap();
        let path = data.path().join("koe-settings.json");
        let store: Arc<dyn SettingsStore> = Arc::new(JsonSettingsStore::new(path));

        let work = tempfile::tempdir().unwrap();
        let file = work.path().join("doomed.txt");
        std::fs::write(&file, b"x").unwrap();
        let policy = PermissionPolicy {
            allowed_folders: vec![AllowedFolder {
                path: work.path().canonicalize().unwrap().to_string_lossy().into_owned(),
                allow_danger: true,
            }],
            ..Default::default()
        };
        store
            .save(&AppSettings { permission_policy: policy, ..AppSettings::default() })
            .expect("seed settings");
        let provider: Arc<dyn PolicyProvider> = Arc::new(SettingsPolicyProvider(Arc::clone(&store)));

        let io = MockIo::new(ApprovalOutcome::Declined); // would block if gated
        run_with_policy(
            &io,
            registry_with("delete_file"),
            call("delete_file", serde_json::json!({ "path": file.to_str().unwrap() })),
            provider,
        )
        .await;
        assert_eq!(io.phases(), vec!["start", "done"], "allow_danger must auto-run via the real store");
    }
}
