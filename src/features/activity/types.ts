// Activity feature — the contract between the Rust backend and the Activity UI.
//
// These shapes are the source of truth that the backend tool_dispatcher
// (koe-2gy) and approval_gate (koe-1vi) MUST emit/accept. They are defined here
// (frontend) first so the UI is not dead code: a dev-only mock emitter and the
// unit tests exercise the exact same shapes the backend will produce.

/** Phase of a single tool invocation. `start`/`progress` are "active". */
export type ToolPhase = "start" | "progress" | "done" | "error";

/**
 * Emitted by the backend on the `tool-event` channel for every step of a tool
 * invocation. One logical tool call is identified by `actionId`; the backend
 * issues a globally monotonic `sequence` so the UI can order events that arrive
 * out of order, and a unique `eventId` so duplicates can be dropped.
 *
 * Payloads are pre-redacted by the backend: `displaySummary` / `detail` must
 * never contain the API key, absolute paths, or PII (see CLAUDE.md).
 */
export interface ToolEvent {
  /** Unique per emit. Primary de-duplication key. */
  eventId: string;
  /** Groups start/progress/done/error of one tool call (= Realtime call_id). */
  actionId: string;
  /** Globally monotonic counter. Used only for display ordering. */
  sequence: number;
  /** Tool name, e.g. "web_search". */
  tool: string;
  phase: ToolPhase;
  /** Backend epoch milliseconds. */
  timestamp: number;
  /** Human-safe, redacted one-line summary of what the tool is doing. */
  displaySummary: string;
  /** Optional redacted extra detail. */
  detail?: string;
  /** Optional progress in the inclusive range [0, 1]. */
  progress?: number;
}

/**
 * Phase of a `thinking-event` disclosure. M1 (koe-sua.1) emits only `"deciding"`
 * — the model has chosen its next verifiable action and is about to take it. The
 * union is intentionally narrow: it carries exactly what the backend emits today,
 * so a future milestone (e.g. a post-action `"reflecting"`) extends it explicitly
 * rather than leaving an unimplemented value documented as live.
 */
export type ThinkingPhase = "deciding";

/**
 * Emitted by the backend on the `thinking-event` channel in the 300–700ms
 * "thinking window" just BEFORE a tool runs — and, by construction, before that
 * tool's `tool-event` phase=start: the backend emits this synchronously in the
 * read loop before it spawns the dispatch that produces the tool-event, so a
 * disclosure always precedes the action it describes. This is koe's glass-box M1
 * (koe-sua.1): the operator sees *what koe is about to do and why* instead of a
 * silent pause.
 *
 * Verifiable-action-first: `plan` is a redacted, tool-derived one-line of the
 * NEXT action ("ウェブを検索しています"); `tool` / `source` are the verifiable act
 * (which tool, what kind of source). The model's raw chain-of-thought is NEVER
 * included — disclosed CoT can be up to 36% unfaithful (Turpin et al. 2305.04388),
 * so koe discloses checkable behaviour, not narration. Payloads are pre-redacted
 * by the backend (no API key / absolute path / PII / tool arguments — see
 * CLAUDE.md).
 *
 * Same ordering/dedup discipline as {@link ToolEvent}: `eventId` de-duplicates,
 * `sequence` (the SAME globally-monotonic counter ToolEvent uses) orders, and
 * `actionId` ties a disclosure to the tool call it precedes.
 */
export interface ThinkingEvent {
  /** Unique per emit. Primary de-duplication key. */
  eventId: string;
  /**
   * Ties this disclosure to the tool call it precedes (= the upcoming
   * ToolEvent.actionId / Realtime call_id).
   */
  actionId: string;
  /**
   * Globally monotonic counter, shared with ToolEvent.sequence. Because the
   * backend mints this BEFORE it dispatches, a disclosure's sequence is always
   * below the `start` of the tool it precedes. Used only for display ordering.
   */
  sequence: number;
  phase: ThinkingPhase;
  /** Redacted, human-safe one-line of the NEXT action. Never raw chain-of-thought. */
  plan: string;
  /**
   * The verifiable tool about to run, e.g. "web_search". Absent for a disclosure
   * with no concrete tool.
   */
  tool?: string;
  /**
   * Coarse, redacted kind of source consulted/produced, e.g. "web" / "ファイル".
   * Absent when the action consults no external source.
   */
  source?: string;
  /**
   * Reserved for the calibrated discrete confidence label (koe-sua.2). Always
   * unset in M1 — the calibration layer that would earn a trustworthy label does
   * not exist yet, so the backend never fabricates one here.
   */
  confidence?: string;
  /** Backend epoch milliseconds. */
  timestamp: number;
}

/**
 * Risk tier of an operation (see CLAUDE.md safety gate). This union is a
 * forward-looking superset: in M1 the backend only ever emits an
 * `ApprovalRequest` for **DANGER** (the 30s human gate). CAUTION is
 * notify-only — it rides a non-blocking `tool-event` (with a `detail` note) and
 * never produces an `ApprovalRequest` (user decision `koe-caution-tier`). The
 * `"CAUTION"` member is retained for a future milestone that may surface CAUTION
 * in the approval UI.
 */
export type ApprovalRisk = "CAUTION" | "DANGER";

/**
 * Emitted by the backend on the `tool-approval-required` channel when a
 * **DANGER** operation needs a human decision (M1 emits this for DANGER only;
 * CAUTION runs immediately — see `ApprovalRisk`). The UI must echo `approvalId`
 * back via `resolve_tool_approval` so the backend can route the answer to the
 * exact pending oneshot. Anything not answered by `deadlineAt` is treated as
 * declined by the backend (fail-closed, 30s timeout).
 */
export interface ApprovalRequest {
  /** Correlation id; echoed back in `resolve_tool_approval`. */
  approvalId: string;
  tool: string;
  risk: ApprovalRisk;
  /** Redacted description of the operation awaiting approval. */
  displaySummary: string;
  /** Backend epoch milliseconds at which the oneshot times out. */
  deadlineAt: number;
  /** Globally monotonic counter (shared space with ToolEvent.sequence). */
  sequence: number;
}

export type ApprovalDecision = "approve" | "deny";

/**
 * Raw connection state reported by the backend on `session-status`.
 * `reconnecting` (koe-byf) is emitted by the session supervisor while it
 * exponential-backoff-retries a recoverable transport drop; the session is NOT
 * over (neither `connected` nor a terminal `idle`/`error`).
 */
export type SessionConnState =
  | "idle"
  | "connecting"
  | "connected"
  | "reconnecting"
  | "error";

export interface SessionStatusEvent {
  state: SessionConnState;
  /** Redacted error message when `state === "error"`. */
  error?: string;
  sequence: number;
}

/**
 * A point-in-time view of this month's spend + budget state (koe-9xi). Mirrors
 * Rust `cost_tracker::CostSnapshot`; field names are snake_case (the backend uses
 * no serde rename). Arrives two ways, both folded through the SAME store guard:
 * the `get_cost_snapshot` command (pull, on mount) and the `cost-update` event
 * (push, on each usage frame).
 *
 * Invariant: `over_budget` is decided in Rust by u64 comparison. The UI MUST show
 * `over_budget` / `used_usd` / `remaining_usd` directly and MUST NOT recompute the
 * over-budget state from the f64 values (judge in u64, display in f64). The
 * `*_nanodollars` integers are carried for completeness (and the M4 prepaid UI),
 * but exceed `Number.MAX_SAFE_INTEGER` at extreme values — never do math on them
 * in JS; render the `*_usd` fields the backend already computed.
 */
export interface CostSnapshot {
  /** Accounting month, YYYYMM. */
  month: number;
  /** Spend so far this month (nanodollars). Informational — render `used_usd`. */
  used_nanodollars: number;
  /** Cap in nanodollars, or null when the budget is disabled (unlimited). */
  limit_nanodollars: number | null;
  /** Whether a hard cap is active. */
  enabled: boolean;
  /** Reached/exceeded the cap (u64 `>=`, decided in Rust). false when unlimited. */
  over_budget: boolean;
  /** Monotonic sequence (shared counter); the store drops a stale lower one. */
  sequence: number;
  /** Display-only USD (used). */
  used_usd: number;
  /** Display-only USD remaining to the cap, or null when unlimited. */
  remaining_usd: number | null;
}

/**
 * Status shown to the user. Derived from connection state + whether any tool is
 * actively running + sticky last error. Labels map to CLAUDE.md's
 * 待機 / 準備 / 会話 / 作業 / エラー.
 */
export type DisplayStatus =
  | "idle" // 待機
  | "connecting" // 準備
  | "reconnecting" // 再接続中 (koe-byf)
  | "conversing" // 会話
  | "working" // 作業
  | "error"; // エラー

/** Live view of one tool invocation, folded from its ToolEvents. */
export interface ActionState {
  actionId: string;
  tool: string;
  phase: ToolPhase;
  startedAt: number;
  updatedAt: number;
  displaySummary: string;
  detail?: string;
  progress?: number;
  /** Highest sequence applied to this action; guards against stale updates. */
  lastSequence: number;
  /**
   * Whether a real `start` event has been folded in. Lets a late-arriving
   * `start` (delivered after `done`/`error` under async concurrency) correct
   * `startedAt` to the true start time without resurrecting the action.
   */
  hasSeenStart: boolean;
}
