// Activity store — folds the backend's `tool-event` / `tool-approval-required`
// / `session-status` streams into the state the operator console renders.
//
// Design notes (Codex R-A/R-B + workflow review):
//  - De-duplicate by `eventId` (NOT by `sequence`), so a late `done` that
//    carries a lower sequence than some unrelated later event is still applied
//    and the LIVE indicator clears.
//  - Order the visible log by `sequence`.
//  - Fold per-`actionId`, guarding each action against *stale* updates with its
//    own `lastSequence` (a single action's events are monotonic), so an
//    out-of-order older event cannot resurrect a finished action.
//  - koe runs continuously, so the dedup set and the action map are bounded to
//    the retained-event window — they must not grow without limit.

import { create } from "zustand";

import type {
  ActionState,
  ApprovalRequest,
  DisplayStatus,
  ProviderErrorEvent,
  SessionConnState,
  SessionStatusEvent,
  ThinkingEvent,
  ToolEvent,
} from "./types";

/** Max number of tool events retained in the visible log. */
export const EVENT_CAP = 100;

/**
 * Max number of thinking disclosures retained in the visible trace (glass-box
 * M1, koe-sua.1). Smaller than {@link EVENT_CAP}: the trace is a short "what koe
 * is thinking right now" window, not a full audit log, and is bounded so a
 * continuously-running session cannot grow it without limit.
 */
export const THINKING_CAP = 50;

/**
 * Hard cap on the action map. Completed actions prune out via the event window,
 * but *active* ones are kept even off-window — so a broken/malicious backend
 * that emits `start` without ever sending `done`/`error` could grow the map
 * without limit. This cap evicts the oldest actions as a backstop. Set well
 * above any realistic concurrent-tool count.
 */
export const MAX_ACTIONS = 256;

/**
 * Max number of provider/server errors retained (koe-nal). Small: these are
 * rare, high-signal rows ("session.update rejected"), not a stream — but a
 * misbehaving server must still not grow the list without limit.
 */
export const PROVIDER_ERROR_CAP = 20;

interface ActivityState {
  connState: SessionConnState;
  /** Sticky error message; cleared when a *newer* non-error status arrives. */
  lastError: string | null;
  /** Visible log, ordered ascending by `sequence`, capped at {@link EVENT_CAP}. */
  events: ToolEvent[];
  /** De-duplication set of seen `eventId`s, bounded to the retained window. */
  seenEventIds: Set<string>;
  /** Live/recent actions keyed by `actionId` (completed ones prune out). */
  actions: Map<string, ActionState>;
  /** Pending approvals, FIFO. The head is the one shown in the modal. */
  approvalQueue: ApprovalRequest[];
  /**
   * Live thinking trace, ordered ascending by `sequence`, capped at
   * {@link THINKING_CAP} (glass-box M1, koe-sua.1).
   */
  thinking: ThinkingEvent[];
  /** De-duplication set of seen thinking `eventId`s, bounded to the retained window. */
  seenThinkingIds: Set<string>;
  /**
   * Non-benign provider/server errors (koe-nal), ordered ascending by
   * `sequence`, capped at {@link PROVIDER_ERROR_CAP}. Sticky across a session
   * (an error explaining why tools went quiet must outlive the moment), cleared
   * when a NEW session starts connecting.
   */
  providerErrors: ProviderErrorEvent[];
  /** De-duplication set of seen provider-error `eventId`s, bounded to the window. */
  seenProviderErrorIds: Set<string>;
  /**
   * The `session-status` sequence at the last clear-on-connecting. Tauri
   * channels are independent, so an error EMITTED by the old session (below
   * the backend's generation gate) can still be DELIVERED after the new
   * session's `connecting` cleared the strip — its lower sequence identifies
   * it as stale (the backend stamps the shared counter at emit time), and
   * `ingestProviderError` drops it (koe-nal R-C).
   */
  providerErrorClearSequence: number;
  /** Highest `sequence` seen across all tool events. */
  lastSequence: number;
  /** Highest `sequence` seen across session-status events (own counter space). */
  lastSessionSequence: number;

  ingestToolEvent: (event: ToolEvent) => void;
  ingestThinkingEvent: (event: ThinkingEvent) => void;
  ingestProviderError: (event: ProviderErrorEvent) => void;
  setSessionStatus: (status: SessionStatusEvent) => void;
  enqueueApproval: (request: ApprovalRequest) => void;
  dequeueApproval: (approvalId: string) => void;
  reset: () => void;
}

function isActivePhase(phase: ActionState["phase"]): boolean {
  return phase === "start" || phase === "progress";
}

function isTerminalPhase(phase: ActionState["phase"]): boolean {
  return phase === "done" || phase === "error";
}

function initialState() {
  return {
    connState: "idle" as SessionConnState,
    lastError: null,
    events: [] as ToolEvent[],
    seenEventIds: new Set<string>(),
    actions: new Map<string, ActionState>(),
    approvalQueue: [] as ApprovalRequest[],
    thinking: [] as ThinkingEvent[],
    seenThinkingIds: new Set<string>(),
    providerErrors: [] as ProviderErrorEvent[],
    seenProviderErrorIds: new Set<string>(),
    providerErrorClearSequence: -1,
    lastSequence: 0,
    // -1 so a backend whose status sequence starts at 0 is not ignored.
    lastSessionSequence: -1,
  };
}

export const useActivityStore = create<ActivityState>((set) => ({
  ...initialState(),

  ingestToolEvent: (event) =>
    set((state) => {
      if (state.seenEventIds.has(event.eventId)) {
        return state; // duplicate — ignore
      }

      // Insert into the log keeping ascending `sequence` order, then cap.
      const events = [...state.events, event].sort((a, b) => a.sequence - b.sequence);
      if (events.length > EVENT_CAP) {
        events.splice(0, events.length - EVENT_CAP);
      }

      // The dedup set tracks only the retained window (bounded memory), so a
      // genuinely old event can slip past the dedup check and be re-ingested.
      const retainedEventIds = new Set(events.map((e) => e.eventId));
      const retainedActionIds = new Set(events.map((e) => e.actionId));

      // Fold into the per-action view — but ONLY for events recent enough to
      // remain in the visible window. An event so old it was immediately evicted
      // must not create or resurrect an action (which would otherwise leave
      // phantom LIVE work on screen forever, since no terminal event follows a
      // replayed old event).
      const actions = new Map(state.actions);
      if (retainedEventIds.has(event.eventId)) {
        const existing = actions.get(event.actionId);
        if (!existing) {
          actions.set(event.actionId, {
            actionId: event.actionId,
            tool: event.tool,
            phase: event.phase,
            startedAt: event.timestamp,
            updatedAt: event.timestamp,
            displaySummary: event.displaySummary,
            detail: event.detail,
            progress: event.progress,
            lastSequence: event.sequence,
            hasSeenStart: event.phase === "start",
          });
        } else if (event.phase === "start" && !existing.hasSeenStart) {
          // A real `start` arrived after a done/error (out-of-order delivery):
          // correct startedAt to the true start time WITHOUT changing the phase,
          // so the action is not resurrected as active.
          actions.set(event.actionId, {
            ...existing,
            startedAt: event.timestamp,
            hasSeenStart: true,
          });
        } else if (event.sequence > existing.lastSequence) {
          // Strictly newer (within this action) — advance. `>` not `>=` so a
          // re-emitted same-sequence event cannot resurrect a finished action.
          actions.set(event.actionId, {
            ...existing,
            tool: event.tool,
            phase: event.phase,
            updatedAt: event.timestamp,
            displaySummary: event.displaySummary,
            detail: event.detail ?? existing.detail,
            // Clear progress once terminal so a completed action keeps no stale %.
            progress: isTerminalPhase(event.phase)
              ? undefined
              : (event.progress ?? existing.progress),
            lastSequence: event.sequence,
          });
        }
        // else: stale within this action — keep the newer phase.
      }

      // Bound memory: completed actions that have scrolled out of the log are
      // dropped (active actions are always kept, even if their start scrolled off).
      for (const [id, action] of actions) {
        if (!isActivePhase(action.phase) && !retainedActionIds.has(id)) {
          actions.delete(id);
        }
      }
      // Backstop: even active actions are capped, so a backend that emits only
      // `start` (no terminal) cannot grow the map without bound. Evict the
      // oldest-started actions first.
      if (actions.size > MAX_ACTIONS) {
        const oldest = [...actions.values()]
          .sort((a, b) => a.startedAt - b.startedAt)
          .slice(0, actions.size - MAX_ACTIONS);
        for (const action of oldest) {
          actions.delete(action.actionId);
        }
      }

      // A disclosure stays visible through its action's EXECUTION — a perceptible
      // window for the operator to read it and decide whether to intervene — and
      // clears when the action COMPLETES. Clearing on the terminal event (not on
      // `start`) avoids two failure modes: a disclosure that lingers after the tool
      // finishes (R-B / Codex Cloud), and a disclosure that collapses to a ~0ms
      // flicker because the backend emits it immediately before dispatch, so `start`
      // follows within ~ms (cr R-B.5). Only rebuild when something is removed.
      const clearsThinking =
        isTerminalPhase(event.phase) &&
        state.thinking.some((t) => t.actionId === event.actionId);
      const thinking = clearsThinking
        ? state.thinking.filter((t) => t.actionId !== event.actionId)
        : state.thinking;
      const seenThinkingIds = clearsThinking
        ? new Set(thinking.map((t) => t.eventId))
        : state.seenThinkingIds;

      return {
        ...state,
        seenEventIds: retainedEventIds,
        events,
        actions,
        thinking,
        seenThinkingIds,
        lastSequence: Math.max(state.lastSequence, event.sequence),
      };
    }),

  // Fold a thinking disclosure (glass-box M1, koe-sua.1) into the live trace.
  // A flat, append-and-sort trace — NOT folded per action like tool events —
  // because a disclosure is a point-in-time "about to do X", not a lifecycle.
  // Same dedup/order discipline as tool events: drop a duplicate `eventId`,
  // order by `sequence`, cap, and bound the dedup set to the retained window so
  // a continuously-running session cannot grow memory without limit.
  ingestThinkingEvent: (event) =>
    set((state) => {
      if (state.seenThinkingIds.has(event.eventId)) {
        return state; // duplicate — ignore
      }
      // A disclosure is valid until its action COMPLETES. If the action already
      // exists AND is terminal (done/error) — a replay, or a disclosure delivered
      // after its tool-event on the separate, unordered Tauri channel — the "about
      // to" is stale, so drop it (Codex Cloud P2). An ACTIVE action keeps its
      // disclosure: the intent is still accurate while the tool runs, and it clears
      // on the terminal tool-event above.
      const existingAction = state.actions.get(event.actionId);
      if (existingAction && isTerminalPhase(existingAction.phase)) {
        return state;
      }
      const thinking = [...state.thinking, event].sort((a, b) => a.sequence - b.sequence);
      if (thinking.length > THINKING_CAP) {
        thinking.splice(0, thinking.length - THINKING_CAP);
      }
      // Track only the retained window (bounded memory), matching the tool-event
      // dedup discipline — a disclosure so old it was evicted may slip back in,
      // which is harmless for a display-only trace.
      const seenThinkingIds = new Set(thinking.map((e) => e.eventId));
      return { ...state, thinking, seenThinkingIds };
    }),

  // Same dedup/order/cap discipline as the thinking trace (koe-nal). No action
  // correlation: a provider error is session-scoped, not tied to a tool call.
  ingestProviderError: (event) =>
    set((state) => {
      if (state.seenProviderErrorIds.has(event.eventId)) {
        return state; // duplicate — ignore
      }
      // A late-DELIVERED error from before the last clear-on-connecting is
      // stale (old session) — drop it so it cannot pollute the new session's
      // strip (see providerErrorClearSequence).
      if (event.sequence <= state.providerErrorClearSequence) {
        return state;
      }
      const providerErrors = [...state.providerErrors, event].sort(
        (a, b) => a.sequence - b.sequence,
      );
      if (providerErrors.length > PROVIDER_ERROR_CAP) {
        providerErrors.splice(0, providerErrors.length - PROVIDER_ERROR_CAP);
      }
      const seenProviderErrorIds = new Set(providerErrors.map((e) => e.eventId));
      return { ...state, providerErrors, seenProviderErrorIds };
    }),

  setSessionStatus: (status) =>
    set((state) => {
      // Ignore stale status: a late `connected` must not clear a newer `error`.
      if (status.sequence <= state.lastSessionSequence) {
        return state;
      }
      // Drop stale pending disclosures whenever there is nothing "about to happen":
      // a stopped (idle) or failed (error) session, OR a reconnect (koe-byf) — a
      // recoverable transport drop aborts the in-flight tool dispatches, so their
      // "これから〜します" disclosures would never be cleared by a completion and
      // would orphan in the window across the reconnect. `connecting`/`connected`
      // leave the window intact.
      const clearThinking =
        status.state === "idle" ||
        status.state === "error" ||
        status.state === "reconnecting";
      // Provider errors are stickier than the thinking window: they explain why
      // tools went quiet, so they survive idle/error/reconnecting as post-mortem
      // context and clear only when the operator starts a NEW session
      // (`connecting`). Reconnects also re-send session.update, but the strip is
      // deliberately kept across them — a re-rejected update simply emits a
      // fresh row (new eventId/sequence), so nothing stale accumulates (koe-nal).
      const clearProviderErrors = status.state === "connecting";
      return {
        ...state,
        connState: status.state,
        lastError: status.state === "error" ? (status.error ?? "unknown error") : null,
        lastSessionSequence: status.sequence,
        ...(clearThinking ? { thinking: [], seenThinkingIds: new Set<string>() } : {}),
        ...(clearProviderErrors
          ? {
              providerErrors: [],
              seenProviderErrorIds: new Set<string>(),
              // Everything emitted up to this status is the OLD session's —
              // drop it even if its delivery straggles in after this clear.
              providerErrorClearSequence: status.sequence,
            }
          : {}),
      };
    }),

  enqueueApproval: (request) =>
    set((state) => {
      if (state.approvalQueue.some((a) => a.approvalId === request.approvalId)) {
        return state; // duplicate approval id — ignore
      }
      return { ...state, approvalQueue: [...state.approvalQueue, request] };
    }),

  dequeueApproval: (approvalId) =>
    set((state) => ({
      ...state,
      approvalQueue: state.approvalQueue.filter((a) => a.approvalId !== approvalId),
    })),

  reset: () => set(() => initialState()),
}));

// --- Derived selectors (pure; usable as `useActivityStore(selectX)`) --------

/** Actions currently running (phase start/progress), oldest first. */
export function selectActiveActions(state: ActivityState): ActionState[] {
  return [...state.actions.values()]
    .filter((a) => isActivePhase(a.phase))
    .sort((a, b) => a.startedAt - b.startedAt);
}

/**
 * Recent thinking disclosures, newest first — for the live "考えていること" trace
 * (glass-box M1, koe-sua.1). The view slices the head to show only the freshest
 * few; the store keeps the rest within {@link THINKING_CAP}.
 */
export function selectRecentThinking(state: ActivityState): ThinkingEvent[] {
  return [...state.thinking].reverse();
}

/**
 * Recent provider/server errors, newest first (koe-nal) — for the ActivityLog's
 * error strip. The view slices the head; the store keeps the rest within
 * {@link PROVIDER_ERROR_CAP}.
 */
export function selectRecentProviderErrors(state: ActivityState): ProviderErrorEvent[] {
  return [...state.providerErrors].reverse();
}

/**
 * The user-facing status, derived from connection state + active work + sticky
 * error. Maps to 待機 / 準備 / 会話 / 作業 / エラー.
 */
export function selectDisplayStatus(state: ActivityState): DisplayStatus {
  if (state.connState === "error" || state.lastError) {
    return "error";
  }
  switch (state.connState) {
    case "idle":
      return "idle";
    case "connecting":
      return "connecting";
    // koe-byf: a recoverable transport drop is being retried — surface it as its own
    // status (再接続中), not idle/conversing, so the operator sees the session is
    // recovering rather than a frozen "会話". (setSessionStatus also clears the
    // thinking window on reconnecting, since the in-flight dispatches were aborted.)
    case "reconnecting":
      return "reconnecting";
    case "connected":
      return selectActiveActions(state).length > 0 ? "working" : "conversing";
    default:
      return "idle";
  }
}
