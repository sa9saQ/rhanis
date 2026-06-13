//! Human-in-the-loop approval gate (rhanis-1vi).
//!
//! Classifies every tool call into one of three risk tiers (CLAUDE.md safety
//! gate) and, for the tiers that need confirmation, asks the operator before the
//! tool runs:
//!
//! | tier    | tools                                            | flow                     |
//! |---------|--------------------------------------------------|--------------------------|
//! | SAFE    | web_search / read_file / take_screenshot / write_note | run immediately     |
//! | CAUTION | write_file / open_url / open_app                 | notify, then run now      |
//! | DANGER  | run_command / delete_file / external_upload      | confirm before running   |
//!
//! Per the user decision (memory `rhanis-caution-tier`): CAUTION is **notify-only**
//! — the dispatcher emits a non-blocking `tool-event` notification and runs the
//! tool immediately. Only DANGER goes through the human gate below.
//!
//! The confirmation is fail-closed: a request that is denied, times out (30s),
//! or whose channel is dropped resolves to [`ApprovalOutcome::Declined`]. Only an
//! explicit `approve` yields [`ApprovalOutcome::Approved`].
//!
//! ## Frontend contract (src/features/activity/types.ts — the source of truth)
//! - emit `tool-approval-required` with
//!   `{ approvalId, tool, risk: "DANGER", displaySummary, deadlineAt, sequence }`
//!   (M1 emits DANGER only; "CAUTION" stays reserved in the union — see types.ts)
//! - command `resolve_tool_approval` accepting `{ approvalId, decision: "approve"|"deny" }`,
//!   routed to the matching pending request by `approvalId`. Unknown / already
//!   resolved / timed-out ids are rejected (fail-closed) — a stale click can
//!   never approve the wrong operation.
//!
//! ## Wiring status
//! [`ApprovalGate::request_approval`] and [`classify`] are the API the
//! tool_dispatcher (rhanis-2gy) will call; they have no in-crate caller yet, so they
//! carry `#[allow(dead_code)]` naming that consumer (same interface-first
//! convention as `secret_store::SecretStore::get_api_key`). The
//! `resolve_tool_approval` command IS wired into `lib.rs` now, so the frontend
//! ApprovalModal's round-trip works the moment a request is emitted.
//!
//! transaction N/A · idempotency_key N/A (in-memory approval routing, not billing).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;

use crate::events::SequenceCounter;

/// Default human-decision deadline. After this the request fails closed.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// Maximum number of DANGER approvals that may be PENDING (awaiting an operator
/// decision) at once. A new DANGER request that would exceed this is refused
/// immediately — fail-closed (never auto-approved) — instead of opening yet
/// another modal. This caps the approval gate's own state against a burst of
/// DANGER `function_call`s from a malicious/compromised model server (rhanis-rxh):
///
/// - **Modal flood** — structurally bounded: at most this many
///   `tool-approval-required` modals ever reach the operator at once.
/// - **Approval-map growth** — bounded: the `pending` map cannot grow without
///   bound under a sustained DANGER burst.
/// - **Dispatch-slot starvation** — *partially* mitigated, NOT fully closed:
///   each pending DANGER holds one in-flight dispatch slot for up to the 30s
///   deadline (tool_dispatcher). In steady state, capping pending approvals far
///   below the dispatch cap (`MAX_INFLIGHT_DISPATCHES` = 64, rhanis-wj2) keeps most
///   slots free. BUT this cap is enforced inside [`ApprovalGate::register`],
///   which a dispatch task only reaches AFTER session_manager has already
///   `spawn`ed it onto the in-flight JoinSet — so a fast back-to-back burst can
///   transiently fill all 64 slots before the over-cap tasks reach `register`
///   and decline, briefly skipping legitimate SAFE/CAUTION calls. Fully closing
///   that race needs a risk-aware admission BEFORE spawn (refuse an over-cap
///   DANGER call without consuming a slot); tracked as follow-up rhanis-e2b.
///
/// Chosen generously enough that a realistic batch of genuine DANGER operations
/// (e.g. "delete these few files") is never refused, yet far below the dispatch
/// cap so steady-state slot pressure stays low.
const MAX_PENDING_APPROVALS: usize = 8;

/// Hard cap on the redacted summary length before it crosses to the WebView.
/// Defense-in-depth: the caller (rhanis-2gy tool_dispatcher) owns redaction, but a
/// cap bounds a pathological/oversized summary from a misbehaving caller so it
/// cannot bloat the IPC payload. This is NOT a substitute for redaction.
const MAX_SUMMARY_LEN: usize = 500;

/// Truncates `s` to at most [`MAX_SUMMARY_LEN`] bytes on a char boundary,
/// appending an ellipsis when it had to cut. Defense-in-depth only.
fn truncate_summary(s: &str) -> String {
    if s.len() <= MAX_SUMMARY_LEN {
        return s.to_string();
    }
    let mut end = MAX_SUMMARY_LEN;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    let mut out = s[..end].to_string();
    out.push('…');
    out
}

/// Risk tier of a tool call. `SAFE` never reaches the operator; `CAUTION` /
/// `DANGER` serialize to the exact strings the frontend `ApprovalRisk` union
/// expects (`"CAUTION"` / `"DANGER"`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum ApprovalRisk {
    Safe,
    Caution,
    Danger,
}

impl ApprovalRisk {
    /// Whether this tier must be confirmed by a human (the 30s gate) before the
    /// tool runs. Per the user decision (memory `rhanis-caution-tier`): **only
    /// DANGER** gates. SAFE runs immediately with no notification; CAUTION emits
    /// a non-blocking `tool-event` notification and then runs immediately (it
    /// does NOT wait for approval). Consumed by the tool_dispatcher (rhanis-2gy) to
    /// route SAFE/CAUTION → run now vs DANGER → `request_approval`.
    pub fn requires_approval(self) -> bool {
        matches!(self, ApprovalRisk::Danger)
    }
}

/// The operator's decision, deserialized from the frontend's `"approve"` /
/// `"deny"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ApprovalDecision {
    Approve,
    Deny,
}

/// Result of awaiting a decision. Everything except an explicit approve is
/// `Declined` (fail-closed).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalOutcome {
    Approved,
    Declined,
}

/// Classifies a tool name into its risk tier.
///
/// An **unknown** tool is classified `DANGER` (fail-closed): the gate must never
/// silently auto-run something it does not recognise. `run_command` is always
/// DANGER here; the shell DENY/ALLOW list is enforced separately at execution
/// time (tool_dispatcher, rhanis-2gy), so a blocked command is rejected outright
/// rather than merely prompting.
///
/// Consumed by the tool_dispatcher (rhanis-2gy).
pub fn classify(tool: &str) -> ApprovalRisk {
    match tool {
        "web_search" | "read_file" | "take_screenshot" | "write_note" => ApprovalRisk::Safe,
        "write_file" | "open_url" | "open_app" => ApprovalRisk::Caution,
        "run_command" | "delete_file" | "external_upload" => ApprovalRisk::Danger,
        // Unknown tool → require the strongest confirmation (fail-closed).
        _ => ApprovalRisk::Danger,
    }
}

/// Payload emitted on `tool-approval-required`. Field names are camelCased to
/// match `ApprovalRequest` in `src/features/activity/types.ts`.
///
/// `Clone` is required by `tauri::Emitter::emit` (it clones the payload per
/// target window), so it stays despite no direct `.clone()` call here.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ApprovalRequestPayload<'a> {
    approval_id: &'a str,
    tool: &'a str,
    risk: ApprovalRisk,
    display_summary: &'a str,
    deadline_at: i64,
    sequence: u64,
}

/// A registered, in-flight approval: the channel that carries the decision back
/// to the awaiting tool task, plus the monotonic instant after which the request
/// is considered expired (the *authoritative* deadline — see [`ApprovalGate::resolve`]).
struct PendingApproval {
    tx: oneshot::Sender<ApprovalDecision>,
    expires_at: Instant,
}

/// RAII guard that removes a pending approval entry when the awaiting future is
/// dropped before it completes — e.g. the dispatch task is aborted on session
/// stop or a budget trip. Without it an aborted DANGER approval would leave an
/// orphaned sender in `pending` (a bounded memory leak plus a stale
/// UI/backend divergence). Removal is idempotent with `await_decision`'s own
/// cleanup, so arming it on every request is safe.
///
/// The `&'a ApprovalGate` borrow means the guard is created and dropped within
/// `request_approval`'s stack frame; it is never moved into a separate task.
struct PendingGuard<'a> {
    gate: &'a ApprovalGate,
    approval_id: String,
}

impl Drop for PendingGuard<'_> {
    fn drop(&mut self) {
        self.gate.remove_pending(&self.approval_id);
    }
}

/// Routes human approval decisions to the tool task awaiting them.
///
/// One `oneshot` channel exists per in-flight request, keyed by `approvalId` in
/// `pending`. `request_approval` inserts the sender and awaits the receiver with
/// a timeout; `resolve` (driven by the Tauri command) removes the sender and
/// delivers the decision. Removing-before-sending means a duplicate resolve or a
/// resolve that races the timeout finds no entry and is rejected.
///
/// ## Threat model (M1)
/// `approvalId`s are random (unguessable) so a limited injection that can call
/// `resolve_tool_approval` but cannot observe the emitted event cannot blind-forge
/// an approval. A *full* WebView compromise that can READ the `tool-approval-required`
/// event can still echo the id back — defending that requires a native (non-WebView)
/// confirmation surface, which is out of M1 scope and tracked for a later milestone.
pub struct ApprovalGate {
    pending: Mutex<HashMap<String, PendingApproval>>,
    /// Shared activity-event sequence (one ordering space with `ToolEvent`).
    seq: Arc<SequenceCounter>,
    timeout: Duration,
    /// Max concurrently-PENDING approvals; a request beyond this fails closed
    /// without opening a modal (see [`MAX_PENDING_APPROVALS`]).
    max_pending: usize,
}

impl ApprovalGate {
    pub fn new(seq: Arc<SequenceCounter>) -> Self {
        Self {
            pending: Mutex::new(HashMap::new()),
            seq,
            timeout: DEFAULT_TIMEOUT,
            max_pending: MAX_PENDING_APPROVALS,
        }
    }

    /// Test/diagnostic constructor with a custom deadline.
    #[cfg(test)]
    fn with_timeout(seq: Arc<SequenceCounter>, timeout: Duration) -> Self {
        Self {
            timeout,
            ..Self::new(seq)
        }
    }

    /// Test/diagnostic constructor with a custom pending-approval cap.
    #[cfg(test)]
    fn with_max_pending(seq: Arc<SequenceCounter>, max_pending: usize) -> Self {
        Self {
            max_pending,
            ..Self::new(seq)
        }
    }

    /// Generates an unguessable 128-bit `approvalId`. A predictable/sequential id
    /// would let an injection that can call `resolve_tool_approval` (but cannot
    /// see the emitted event) blind-guess the pending id and approve a DANGER op.
    fn gen_id() -> String {
        let mut bytes = [0u8; 16];
        // A CSPRNG failure is unrecoverable for a security token; failing loud
        // (panic) is preferable to emitting a guessable id. The panic aborts only
        // this approval's task → it never emits → the awaiting side fails closed.
        getrandom::getrandom(&mut bytes).expect("CSPRNG must be available");
        let mut id = String::from("appr-");
        for b in bytes {
            id.push_str(&format!("{b:02x}"));
        }
        id
    }

    /// Reserves an `approvalId` + a `sequence`, registers a pending oneshot, and
    /// returns the receiver. Split out from [`request_approval`] so the routing
    /// logic is testable without a Tauri `AppHandle`.
    fn register(&self) -> Option<(String, u64, oneshot::Receiver<ApprovalDecision>)> {
        let (tx, rx) = oneshot::channel();
        let expires_at = Instant::now() + self.timeout;
        // A poisoned lock means a prior holder panicked; recover the guard rather
        // than propagating the panic. The worst case is an orphaned pending entry
        // (a sender whose receiver was dropped by the panic): it can never be
        // approved — a later `resolve` finds the receiver gone and returns Err —
        // and it is reclaimed when `resolve` targets it or at process exit. It
        // does NOT self-expire (no `await_decision` is running for it), so this
        // is a bounded memory quirk, never an auto-approval.
        let mut pending = self
            .pending
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        // Pending-approval cap (rhanis-rxh): refuse a new request that would exceed
        // the cap WITHOUT reserving an id/sequence or opening a modal. Checked
        // under the same lock as the insert below, so two concurrent requests can
        // never both slip past the cap. The caller (`request_approval`) fails
        // closed (Declined) on `None`.
        if pending.len() >= self.max_pending {
            return None;
        }
        // Reserve id + sequence only now that the request is admitted, so a
        // refused (over-cap) request leaves no observable trace: it neither
        // advances the activity sequence nor consumes a CSPRNG id.
        let approval_id = Self::gen_id();
        let sequence = self.seq.next();
        pending.insert(approval_id.clone(), PendingApproval { tx, expires_at });
        Some((approval_id, sequence, rx))
    }

    /// Awaits the decision for `approval_id`, enforcing the fail-closed deadline.
    /// On every exit path the pending entry is removed (the timeout/drop paths
    /// remove it here; a successful `resolve` already removed it).
    async fn await_decision(
        &self,
        approval_id: &str,
        rx: oneshot::Receiver<ApprovalDecision>,
    ) -> ApprovalOutcome {
        let outcome = match tokio::time::timeout(self.timeout, rx).await {
            Ok(Ok(ApprovalDecision::Approve)) => ApprovalOutcome::Approved,
            // Explicit deny, OR the sender was dropped without sending (e.g. the
            // app tore down) — both fail closed.
            Ok(Ok(ApprovalDecision::Deny)) | Ok(Err(_)) => ApprovalOutcome::Declined,
            // Deadline elapsed without a decision — fail closed.
            Err(_) => ApprovalOutcome::Declined,
        };
        self.remove_pending(approval_id);
        outcome
    }

    fn remove_pending(&self, approval_id: &str) {
        if let Ok(mut pending) = self.pending.lock() {
            pending.remove(approval_id);
        }
    }

    /// Emits an approval request and awaits the human decision (fail-closed,
    /// 30s). Consumed by the tool_dispatcher (rhanis-2gy) for DANGER tools only
    /// (CAUTION is notify-only and never calls this — rhanis-caution-tier).
    ///
    /// `display_summary` MUST be pre-redacted by the caller — at most the safe
    /// target descriptor of `display_descriptor` (home-relative path / first
    /// command token / URL host, rhanis-whf); never a key, raw absolute path, or
    /// PII. It is shown (after a defensive length cap) in the modal. The cap is
    /// belt-and-suspenders; redaction remains the caller's job.
    pub async fn request_approval(
        &self,
        app: &tauri::AppHandle,
        tool: &str,
        risk: ApprovalRisk,
        display_summary: String,
    ) -> ApprovalOutcome {
        use tauri::Emitter;

        let (approval_id, sequence, rx) = match self.register() {
            Some(reg) => reg,
            // Pending-approval cap reached (rhanis-rxh): refuse this DANGER request
            // without emitting a modal. Fail-closed — the tool does not run — and
            // the in-flight dispatch slot is freed immediately, so a burst of
            // DANGER calls cannot starve legitimate SAFE/CAUTION dispatches or
            // flood the operator with modals.
            None => return ApprovalOutcome::Declined,
        };
        // RAII safety net: if this future is dropped (e.g. the dispatch task is
        // aborted on session stop / budget trip) before `await_decision` removes
        // the entry, the guard removes it so no orphaned sender leaks in
        // `pending`. Removal is idempotent with `await_decision`'s own cleanup.
        let _guard = PendingGuard {
            gate: self,
            approval_id: approval_id.clone(),
        };
        let deadline_at = now_ms().saturating_add(self.timeout_millis());
        let display_summary = truncate_summary(&display_summary);

        let payload = ApprovalRequestPayload {
            approval_id: &approval_id,
            tool,
            risk,
            display_summary: &display_summary,
            deadline_at,
            sequence,
        };
        // Best-effort emit: if it fails the operator never sees the modal, the
        // deadline elapses, and the request fails closed (Declined) — never a
        // silent approve.
        let _ = app.emit("tool-approval-required", payload);

        self.await_decision(&approval_id, rx).await
    }

    /// Delivers a decision to the matching pending request. Fixed, leak-free
    /// error strings. Returns `Err` for an unknown / already-resolved /
    /// expired `approval_id`, or when the awaiting side has already gone away.
    fn resolve(&self, approval_id: &str, decision: ApprovalDecision) -> Result<(), &'static str> {
        // Bound the input: a valid id is "appr-" + 32 hex = 37 chars. Reject
        // anything wildly longer up front (leak-free fixed message), so a
        // misbehaving caller cannot push huge keys at the pending map.
        if approval_id.len() > 64 {
            return Err("unknown approval");
        }
        // Remove first: a second resolve (or a resolve racing the timeout) then
        // finds nothing and is rejected, so a decision is delivered at most once.
        let entry = {
            let mut pending = self
                .pending
                .lock()
                .map_err(|_| "approval gate is unavailable")?;
            pending.remove(approval_id)
        };
        match entry {
            Some(p) => {
                // Hard deadline. `tokio::time::timeout` polls the *receiver*
                // before the timer, so a decision delivered after the deadline
                // but before the awaiting task is re-polled would otherwise leak
                // through as `Approved`. Rejecting an expired decision here (and
                // dropping its sender) guarantees a late `approve` fails closed:
                // the awaiting side then resolves to Declined via timeout / the
                // dropped sender.
                if Instant::now() >= p.expires_at {
                    return Err("approval has expired");
                }
                p.tx.send(decision)
                    .map_err(|_| "approval is no longer awaiting a decision")
            }
            None => Err("unknown approval"),
        }
    }

    #[cfg(test)]
    fn pending_len(&self) -> usize {
        self.pending.lock().unwrap().len()
    }

    /// Test helper: registers an entry whose deadline is already in the past, and
    /// returns the live receiver. Keeping `rx` alive means a rejected `resolve`
    /// can only be due to the hard deadline, not a dropped receiver.
    #[cfg(test)]
    fn register_with_past_deadline(&self) -> (String, oneshot::Receiver<ApprovalDecision>) {
        let approval_id = Self::gen_id();
        let (tx, rx) = oneshot::channel();
        let expires_at = Instant::now()
            .checked_sub(Duration::from_secs(1))
            .unwrap_or_else(Instant::now);
        self.pending
            .lock()
            .unwrap()
            .insert(approval_id.clone(), PendingApproval { tx, expires_at });
        (approval_id, rx)
    }

    /// `self.timeout` in milliseconds, clamped to `i64` for the epoch-ms
    /// `deadlineAt` (our timeouts are seconds-scale, so the clamp never bites).
    fn timeout_millis(&self) -> i64 {
        i64::try_from(self.timeout.as_millis()).unwrap_or(i64::MAX)
    }
}

/// Tauri managed-state wrapper. `lib.rs` constructs one `ApprovalGate` (sharing
/// the process `SequenceCounter`) and the tool_dispatcher (rhanis-2gy) reaches it
/// via `tauri::State<'_, ManagedApprovalGate>`.
pub struct ManagedApprovalGate(pub Arc<ApprovalGate>);

/// Resolves a pending approval. The decision is routed to the exact pending
/// request by `approval_id`; unknown / already-resolved / timed-out ids are
/// rejected (fail-closed). The fixed error strings carry no path/PII.
#[tauri::command]
pub async fn resolve_tool_approval(
    approval_id: String,
    decision: ApprovalDecision,
    gate: tauri::State<'_, ManagedApprovalGate>,
) -> Result<(), String> {
    gate.0
        .resolve(&approval_id, decision)
        .map_err(|e| e.to_string())
}

/// Wall-clock epoch milliseconds for `deadlineAt` (display/countdown only — the
/// authoritative deadline is the `tokio` timeout, not this value). A pre-epoch
/// clock falls back to 0; a far-future clock saturates rather than truncating.
fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gate() -> ApprovalGate {
        ApprovalGate::new(Arc::new(SequenceCounter::new()))
    }

    // ---- classification ------------------------------------------------------

    #[test]
    fn classify_maps_known_tools_to_their_tier() {
        for t in ["web_search", "read_file", "take_screenshot", "write_note"] {
            assert_eq!(classify(t), ApprovalRisk::Safe, "{t} should be SAFE");
        }
        for t in ["write_file", "open_url", "open_app"] {
            assert_eq!(classify(t), ApprovalRisk::Caution, "{t} should be CAUTION");
        }
        for t in ["run_command", "delete_file", "external_upload"] {
            assert_eq!(classify(t), ApprovalRisk::Danger, "{t} should be DANGER");
        }
    }

    #[test]
    fn classify_unknown_tool_is_danger_fail_closed() {
        assert_eq!(classify("rm_rf_everything"), ApprovalRisk::Danger);
        assert_eq!(classify(""), ApprovalRisk::Danger);
    }

    #[test]
    fn only_danger_requires_approval() {
        // Per the user decision (rhanis-caution-tier): SAFE and CAUTION both run
        // immediately (CAUTION emits a non-blocking notification, no gate); only
        // DANGER goes through the 30s human gate.
        assert!(!ApprovalRisk::Safe.requires_approval());
        assert!(!ApprovalRisk::Caution.requires_approval());
        assert!(ApprovalRisk::Danger.requires_approval());
    }

    // ---- risk serializes to the frontend's exact strings ---------------------

    #[test]
    fn risk_serializes_to_uppercase_contract_strings() {
        assert_eq!(
            serde_json::to_string(&ApprovalRisk::Caution).unwrap(),
            "\"CAUTION\""
        );
        assert_eq!(
            serde_json::to_string(&ApprovalRisk::Danger).unwrap(),
            "\"DANGER\""
        );
    }

    #[test]
    fn decision_deserializes_from_frontend_strings() {
        assert_eq!(
            serde_json::from_str::<ApprovalDecision>("\"approve\"").unwrap(),
            ApprovalDecision::Approve
        );
        assert_eq!(
            serde_json::from_str::<ApprovalDecision>("\"deny\"").unwrap(),
            ApprovalDecision::Deny
        );
        assert!(serde_json::from_str::<ApprovalDecision>("\"APPROVE\"").is_err());
    }

    // ---- registration --------------------------------------------------------

    #[test]
    fn register_produces_unique_ids_and_monotonic_sequence() {
        let g = gate();
        let (id0, seq0, _rx0) = g.register().expect("register within cap");
        let (id1, seq1, _rx1) = g.register().expect("register within cap");
        assert_ne!(id0, id1);
        assert_eq!(seq0, 0);
        assert_eq!(seq1, 1);
        assert_eq!(g.pending_len(), 2);
    }

    // ---- resolve routing -----------------------------------------------------

    #[tokio::test]
    async fn resolve_approve_yields_approved_and_clears_pending() {
        let g = gate();
        let (id, _seq, rx) = g.register().expect("register within cap");
        // Decision can be delivered before the await; oneshot buffers it.
        g.resolve(&id, ApprovalDecision::Approve).expect("resolve");
        assert_eq!(g.await_decision(&id, rx).await, ApprovalOutcome::Approved);
        assert_eq!(g.pending_len(), 0);
    }

    #[tokio::test]
    async fn resolve_deny_yields_declined() {
        let g = gate();
        let (id, _seq, rx) = g.register().expect("register within cap");
        g.resolve(&id, ApprovalDecision::Deny).expect("resolve");
        assert_eq!(g.await_decision(&id, rx).await, ApprovalOutcome::Declined);
        assert_eq!(g.pending_len(), 0);
    }

    #[test]
    fn resolve_unknown_id_is_rejected() {
        let g = gate();
        assert_eq!(
            g.resolve("appr-does-not-exist", ApprovalDecision::Approve),
            Err("unknown approval")
        );
    }

    #[test]
    fn resolve_twice_rejects_the_duplicate() {
        let g = gate();
        let (id, _seq, _rx) = g.register().expect("register within cap");
        assert!(g.resolve(&id, ApprovalDecision::Approve).is_ok());
        // Second resolve finds no entry (removed on first) → rejected.
        assert_eq!(
            g.resolve(&id, ApprovalDecision::Approve),
            Err("unknown approval")
        );
    }

    #[tokio::test]
    async fn resolve_after_receiver_dropped_is_rejected() {
        // A resolve that arrives after the awaiting side already gave up (rx
        // dropped) must report failure, not a phantom success.
        let g = gate();
        let (id, _seq, rx) = g.register().expect("register within cap");
        drop(rx);
        assert_eq!(
            g.resolve(&id, ApprovalDecision::Approve),
            Err("approval is no longer awaiting a decision")
        );
    }

    // ---- fail-closed deadline ------------------------------------------------

    #[tokio::test]
    async fn timeout_without_decision_declines() {
        let g = ApprovalGate::with_timeout(
            Arc::new(SequenceCounter::new()),
            Duration::from_millis(20),
        );
        let (id, _seq, rx) = g.register().expect("register within cap");
        // No resolve — the deadline must elapse and decline.
        assert_eq!(g.await_decision(&id, rx).await, ApprovalOutcome::Declined);
        assert_eq!(g.pending_len(), 0);
    }

    #[tokio::test]
    async fn sender_dropped_before_decision_declines() {
        // Simulate the app dropping the pending sender (teardown) without a
        // decision: the awaiting side must fail closed.
        let g = gate();
        let (id, _seq, rx) = g.register().expect("register within cap");
        // Remove + drop the sender out from under the receiver.
        g.remove_pending(&id);
        assert_eq!(g.await_decision(&id, rx).await, ApprovalOutcome::Declined);
    }

    #[tokio::test]
    async fn resolve_after_timeout_reports_unknown() {
        // After the deadline elapses, await_decision removes the pending entry.
        // A late resolve must then find nothing — never a phantom Approved.
        let g = ApprovalGate::with_timeout(
            Arc::new(SequenceCounter::new()),
            Duration::from_millis(15),
        );
        let (id, _seq, rx) = g.register().expect("register within cap");
        assert_eq!(g.await_decision(&id, rx).await, ApprovalOutcome::Declined);
        assert_eq!(
            g.resolve(&id, ApprovalDecision::Approve),
            Err("unknown approval")
        );
    }

    #[test]
    fn resolve_after_hard_deadline_is_rejected_even_with_live_receiver() {
        // The pending entry still exists and its receiver is alive, so the ONLY
        // reason to reject is the elapsed hard deadline — a late `approve` must
        // never surface as Approved (the R-C race: timeout polls rx first).
        let g = gate();
        let (id, _rx) = g.register_with_past_deadline();
        assert_eq!(
            g.resolve(&id, ApprovalDecision::Approve),
            Err("approval has expired")
        );
        // The entry was removed even though it expired.
        assert_eq!(g.pending_len(), 0);
    }

    #[test]
    fn approval_ids_are_unguessable_and_unique() {
        // Random 128-bit ids: not sequential, and distinct across calls.
        let a = ApprovalGate::gen_id();
        let b = ApprovalGate::gen_id();
        assert_ne!(a, b);
        assert!(a.starts_with("appr-"));
        assert_eq!(a.len(), "appr-".len() + 32); // 16 bytes hex
        assert!(!a.ends_with("appr-0"));
    }

    // ---- frontend contract: emit payload serialization -----------------------

    #[test]
    fn payload_serializes_to_frontend_camelcase_keys() {
        // Locks the wire contract with src/features/activity/types.ts: camelCase
        // keys + the uppercase risk string. A rename here would silently break
        // the ApprovalModal, so assert the exact emitted shape.
        let payload = ApprovalRequestPayload {
            approval_id: "appr-7",
            tool: "delete_file",
            risk: ApprovalRisk::Danger,
            display_summary: "delete a file",
            deadline_at: 1234,
            sequence: 9,
        };
        let v = serde_json::to_value(&payload).expect("serialize");
        assert_eq!(v["approvalId"], "appr-7");
        assert_eq!(v["tool"], "delete_file");
        assert_eq!(v["risk"], "DANGER");
        assert_eq!(v["displaySummary"], "delete a file");
        assert_eq!(v["deadlineAt"], 1234);
        assert_eq!(v["sequence"], 9);
        // No snake_case key leaks through.
        assert!(v.get("approval_id").is_none());
        assert!(v.get("display_summary").is_none());
        assert!(v.get("deadline_at").is_none());
    }

    // ---- classification is case-sensitive (fail-closed on variants) ----------

    #[test]
    fn classify_is_case_sensitive_variants_are_danger() {
        // A case variant is not the registered tool name → unknown → DANGER.
        assert_eq!(classify("Write_File"), ApprovalRisk::Danger);
        assert_eq!(classify("WEB_SEARCH"), ApprovalRisk::Danger);
        assert_eq!(classify("Read_File"), ApprovalRisk::Danger);
    }

    // ---- defensive summary cap -----------------------------------------------

    #[test]
    fn truncate_summary_caps_length_on_char_boundary() {
        let long = "x".repeat(1000);
        let t = truncate_summary(&long);
        assert!(t.len() < long.len(), "must shrink");
        assert!(t.chars().count() <= MAX_SUMMARY_LEN + 1, "cap + ellipsis");
        assert!(t.ends_with('…'));
        // A short summary is returned unchanged.
        assert_eq!(truncate_summary("short"), "short");
        // Multi-byte content stays valid UTF-8 (truncates on a char boundary).
        let multi = "あ".repeat(1000);
        let tm = truncate_summary(&multi);
        assert!(tm.is_char_boundary(tm.len()));
    }

    // ---- pending guard (aborted request future) ------------------------------

    #[test]
    fn pending_guard_removes_entry_when_request_future_is_dropped() {
        // Mimics the dispatch task being aborted after the request registered but
        // before a decision: the PendingGuard (held in request_approval's frame)
        // drops and removes the entry, so no orphaned sender leaks in `pending`.
        let g = gate();
        let (id, _seq, _rx) = g.register().expect("register within cap");
        assert_eq!(g.pending_len(), 1);
        {
            let _guard = PendingGuard {
                gate: &g,
                approval_id: id,
            };
            // guard drops at scope end (stands in for the future being dropped)
        }
        assert_eq!(g.pending_len(), 0);
    }

    #[test]
    fn resolve_rejects_oversized_id_input() {
        // The command-level length guard rejects an absurdly long id before it
        // reaches the pending map.
        let g = gate();
        let huge = "x".repeat(100);
        assert_eq!(g.resolve(&huge, ApprovalDecision::Approve), Err("unknown approval"));
    }

    // ---- pending-approval cap (rhanis-rxh: modal-flood + starvation guard) -------

    #[test]
    fn register_refuses_new_requests_at_pending_cap() {
        // At the cap a further register is refused (None) so the caller fails
        // closed without opening another modal; the already-pending entries stay.
        let g = ApprovalGate::with_max_pending(Arc::new(SequenceCounter::new()), 3);
        let _a = g.register().expect("1st within cap");
        let _b = g.register().expect("2nd within cap");
        let _c = g.register().expect("3rd within cap");
        assert!(g.register().is_none(), "4th request must be refused at the cap");
        assert_eq!(g.pending_len(), 3, "a refused request must not be registered");
    }

    #[test]
    fn freeing_a_pending_slot_lets_a_new_request_register() {
        // The cap counts CURRENTLY-pending approvals: once one resolves/expires and
        // frees its slot, a new DANGER request can register again (the cap is a
        // concurrency bound, not a lifetime quota).
        let g = ApprovalGate::with_max_pending(Arc::new(SequenceCounter::new()), 2);
        let (id_a, _s, _rx_a) = g.register().expect("1st within cap");
        let _b = g.register().expect("2nd within cap");
        assert!(g.register().is_none(), "at cap");
        g.remove_pending(&id_a); // a decision arrives / the deadline elapses for A
        assert!(
            g.register().is_some(),
            "a freed slot must allow a new registration"
        );
        assert_eq!(g.pending_len(), 2);
    }

    #[test]
    fn refused_over_cap_request_has_no_side_effect_on_sequence() {
        // A refused (over-cap) request reserves neither an id nor a sequence, so
        // the abuse path cannot advance the shared activity sequence. With cap=1:
        // accept (seq 0) → refuse (no seq.next) → free → accept (seq 1, no gap).
        let g = ApprovalGate::with_max_pending(Arc::new(SequenceCounter::new()), 1);
        let (id0, seq0, _rx0) = g.register().expect("1st within cap");
        assert_eq!(seq0, 0);
        assert!(g.register().is_none(), "at cap → refused");
        g.remove_pending(&id0); // free the only slot
        let (_id1, seq1, _rx1) = g.register().expect("after slot freed");
        assert_eq!(seq1, 1, "the refused request must not have advanced the sequence");
    }
}
