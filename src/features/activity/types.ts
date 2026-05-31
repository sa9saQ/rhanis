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

/** Raw connection state reported by the backend on `session-status`. */
export type SessionConnState = "idle" | "connecting" | "connected" | "error";

export interface SessionStatusEvent {
  state: SessionConnState;
  /** Redacted error message when `state === "error"`. */
  error?: string;
  sequence: number;
}

/**
 * Status shown to the user. Derived from connection state + whether any tool is
 * actively running + sticky last error. Labels map to CLAUDE.md's
 * 待機 / 準備 / 会話 / 作業 / エラー.
 */
export type DisplayStatus =
  | "idle" // 待機
  | "connecting" // 準備
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
