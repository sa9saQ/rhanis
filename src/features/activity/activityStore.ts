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
  /** Highest `sequence` seen across all tool events. */
  lastSequence: number;
  /** Highest `sequence` seen across session-status events (own counter space). */
  lastSessionSequence: number;

  ingestToolEvent: (event: ToolEvent) => void;
  ingestThinkingEvent: (event: ThinkingEvent) => void;
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

      // A disclosure's job ends the moment its action actually starts: once a
      // tool-event for this `actionId` arrives, drop the "about to" disclosure so
      // the thinking window stays EPHEMERAL (the pre-tool 300-700ms window) and
      // never lingers next to the live action or after it completes (R-C). Only
      // rebuild when something is actually removed, to avoid needless churn.
      const hadThinking = state.thinking.some((t) => t.actionId === event.actionId);
      const thinking = hadThinking
        ? state.thinking.filter((t) => t.actionId !== event.actionId)
        : state.thinking;
      const seenThinkingIds = hadThinking
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

  setSessionStatus: (status) =>
    set((state) => {
      // Ignore stale status: a late `connected` must not clear a newer `error`.
      if (status.sequence <= state.lastSessionSequence) {
        return state;
      }
      // A stopped (idle) or failed (error) session has nothing "about to happen":
      // drop stale pending disclosures so the present-tense thinking window never
      // outlives its session (R-B/R-C). `connecting`/`connected` leave it intact.
      const ended = status.state === "idle" || status.state === "error";
      return {
        ...state,
        connState: status.state,
        lastError: status.state === "error" ? (status.error ?? "unknown error") : null,
        lastSessionSequence: status.sequence,
        ...(ended ? { thinking: [], seenThinkingIds: new Set<string>() } : {}),
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
    case "connected":
      return selectActiveActions(state).length > 0 ? "working" : "conversing";
    default:
      return "idle";
  }
}
