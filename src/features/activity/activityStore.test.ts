import { beforeEach, describe, expect, it } from "vitest";

import {
  EVENT_CAP,
  MAX_ACTIONS,
  THINKING_CAP,
  selectActiveActions,
  selectDisplayStatus,
  selectRecentThinking,
  useActivityStore,
} from "./activityStore";
import type { ApprovalRequest, ThinkingEvent, ToolEvent, ToolPhase } from "./types";

function ev(
  partial: Pick<ToolEvent, "eventId" | "actionId" | "sequence" | "phase"> & Partial<ToolEvent>,
): ToolEvent {
  return {
    tool: "web_search",
    timestamp: partial.sequence * 1000,
    displaySummary: "searching the web",
    ...partial,
  };
}

function think(
  partial: Pick<ThinkingEvent, "eventId" | "actionId" | "sequence"> & Partial<ThinkingEvent>,
): ThinkingEvent {
  return {
    phase: "deciding",
    plan: "ウェブを検索しています",
    tool: "web_search",
    source: "web",
    timestamp: partial.sequence * 1000,
    ...partial,
  };
}

function approval(partial: Partial<ApprovalRequest> & { approvalId: string }): ApprovalRequest {
  return {
    tool: "run_command",
    risk: "DANGER",
    displaySummary: "run a shell command",
    deadlineAt: 30_000,
    sequence: 1,
    ...partial,
  };
}

beforeEach(() => {
  useActivityStore.getState().reset();
});

describe("ingestToolEvent — de-duplication and ordering", () => {
  it("ignores a duplicate eventId", () => {
    const s = useActivityStore.getState();
    s.ingestToolEvent(ev({ eventId: "e1", actionId: "a1", sequence: 1, phase: "start" }));
    s.ingestToolEvent(ev({ eventId: "e1", actionId: "a1", sequence: 1, phase: "start" }));
    expect(useActivityStore.getState().events).toHaveLength(1);
  });

  it("orders the log by sequence regardless of arrival order", () => {
    const s = useActivityStore.getState();
    s.ingestToolEvent(ev({ eventId: "e3", actionId: "a1", sequence: 3, phase: "progress" }));
    s.ingestToolEvent(ev({ eventId: "e1", actionId: "a1", sequence: 1, phase: "start" }));
    s.ingestToolEvent(ev({ eventId: "e2", actionId: "a1", sequence: 2, phase: "progress" }));
    expect(useActivityStore.getState().events.map((e) => e.sequence)).toEqual([1, 2, 3]);
  });

  it("caps the log at EVENT_CAP, keeping the highest sequences", () => {
    const s = useActivityStore.getState();
    for (let i = 1; i <= EVENT_CAP + 20; i++) {
      s.ingestToolEvent(ev({ eventId: `e${i}`, actionId: `a${i}`, sequence: i, phase: "done" }));
    }
    const events = useActivityStore.getState().events;
    expect(events).toHaveLength(EVENT_CAP);
    expect(events[0].sequence).toBe(21);
    expect(events[events.length - 1].sequence).toBe(EVENT_CAP + 20);
  });
});

describe("ingestToolEvent — action folding", () => {
  it("marks an action active on start and inactive on done", () => {
    const s = useActivityStore.getState();
    s.ingestToolEvent(ev({ eventId: "e1", actionId: "a1", sequence: 1, phase: "start" }));
    expect(selectActiveActions(useActivityStore.getState())).toHaveLength(1);
    s.ingestToolEvent(ev({ eventId: "e2", actionId: "a1", sequence: 2, phase: "done" }));
    expect(selectActiveActions(useActivityStore.getState())).toHaveLength(0);
  });

  it("does not resurrect an action when a stale start arrives after done", () => {
    const s = useActivityStore.getState();
    s.ingestToolEvent(ev({ eventId: "edone", actionId: "a1", sequence: 5, phase: "done" }));
    s.ingestToolEvent(ev({ eventId: "estart", actionId: "a1", sequence: 3, phase: "start" }));
    const action = useActivityStore.getState().actions.get("a1");
    expect(action?.phase).toBe("done");
    expect(selectActiveActions(useActivityStore.getState())).toHaveLength(0);
  });

  it("tracks the latest progress value", () => {
    const s = useActivityStore.getState();
    s.ingestToolEvent(ev({ eventId: "e1", actionId: "a1", sequence: 1, phase: "start" }));
    s.ingestToolEvent(
      ev({ eventId: "e2", actionId: "a1", sequence: 2, phase: "progress", progress: 0.5 }),
    );
    expect(useActivityStore.getState().actions.get("a1")?.progress).toBe(0.5);
  });

  it("clears progress once an action reaches a terminal phase", () => {
    const s = useActivityStore.getState();
    s.ingestToolEvent(ev({ eventId: "e1", actionId: "a1", sequence: 1, phase: "start" }));
    s.ingestToolEvent(
      ev({ eventId: "e2", actionId: "a1", sequence: 2, phase: "progress", progress: 0.6 }),
    );
    s.ingestToolEvent(ev({ eventId: "e3", actionId: "a1", sequence: 3, phase: "done" }));
    expect(useActivityStore.getState().actions.get("a1")?.progress).toBeUndefined();
  });

  it("corrects startedAt when the real start arrives after done (out-of-order)", () => {
    const s = useActivityStore.getState();
    // done arrives first (later timestamp), then the real start (earlier).
    s.ingestToolEvent(
      ev({ eventId: "edone", actionId: "a1", sequence: 5, phase: "done", timestamp: 9000 }),
    );
    expect(useActivityStore.getState().actions.get("a1")?.startedAt).toBe(9000);
    s.ingestToolEvent(
      ev({ eventId: "estart", actionId: "a1", sequence: 3, phase: "start", timestamp: 1000 }),
    );
    const action = useActivityStore.getState().actions.get("a1");
    expect(action?.startedAt).toBe(1000); // corrected to the true start time
    expect(action?.phase).toBe("done"); // not resurrected
    expect(selectActiveActions(useActivityStore.getState())).toHaveLength(0);
  });

  it("treats an equal-sequence re-emit as stale (no resurrection)", () => {
    const s = useActivityStore.getState();
    s.ingestToolEvent(ev({ eventId: "estart", actionId: "a1", sequence: 5, phase: "start" }));
    s.ingestToolEvent(ev({ eventId: "edone", actionId: "a1", sequence: 6, phase: "done" }));
    // A retry re-emits a same-sequence progress with a fresh eventId.
    s.ingestToolEvent(ev({ eventId: "edup", actionId: "a1", sequence: 6, phase: "progress" }));
    expect(useActivityStore.getState().actions.get("a1")?.phase).toBe("done");
    expect(selectActiveActions(useActivityStore.getState())).toHaveLength(0);
  });

  it("prunes completed actions that have scrolled out of the event window", () => {
    const s = useActivityStore.getState();
    // One completed action, then EVENT_CAP newer events from other actions push
    // its events out of the retained window.
    s.ingestToolEvent(ev({ eventId: "old", actionId: "old", sequence: 1, phase: "done" }));
    for (let i = 1; i <= EVENT_CAP; i++) {
      s.ingestToolEvent(ev({ eventId: `e${i}`, actionId: `a${i}`, sequence: i + 1, phase: "done" }));
    }
    const { actions, seenEventIds } = useActivityStore.getState();
    expect(actions.has("old")).toBe(false); // pruned
    expect(actions.size).toBeLessThanOrEqual(EVENT_CAP);
    expect(seenEventIds.size).toBeLessThanOrEqual(EVENT_CAP);
  });

  it("keeps an active action even after its start event scrolls out of the log", () => {
    const s = useActivityStore.getState();
    s.ingestToolEvent(ev({ eventId: "live", actionId: "live", sequence: 1, phase: "start" }));
    for (let i = 1; i <= EVENT_CAP; i++) {
      s.ingestToolEvent(ev({ eventId: `e${i}`, actionId: `a${i}`, sequence: i + 1, phase: "done" }));
    }
    expect(useActivityStore.getState().actions.has("live")).toBe(true);
    expect(selectActiveActions(useActivityStore.getState())).toHaveLength(1);
  });

  it("does not resurrect an action from a replayed event older than the window", () => {
    const s = useActivityStore.getState();
    // Fill the window with newer events.
    for (let i = 1; i <= EVENT_CAP; i++) {
      s.ingestToolEvent(
        ev({ eventId: `e${i}`, actionId: `a${i}`, sequence: i + 100, phase: "done" }),
      );
    }
    // Replay an OLD start (distinct eventId, sequence below the window).
    s.ingestToolEvent(ev({ eventId: "ancient", actionId: "ancient", sequence: 1, phase: "start" }));
    expect(useActivityStore.getState().actions.has("ancient")).toBe(false);
    expect(selectActiveActions(useActivityStore.getState())).toHaveLength(0);
  });

  it("caps the action map even when a backend only emits start events", () => {
    const s = useActivityStore.getState();
    const total = MAX_ACTIONS + 50;
    for (let i = 1; i <= total; i++) {
      s.ingestToolEvent(ev({ eventId: `e${i}`, actionId: `a${i}`, sequence: i, phase: "start" }));
    }
    const actions = useActivityStore.getState().actions;
    expect(actions.size).toBeLessThanOrEqual(MAX_ACTIONS);
    // The most recent action survives; the oldest is evicted.
    expect(actions.has(`a${total}`)).toBe(true);
    expect(actions.has("a1")).toBe(false);
  });
});

describe("ingestThinkingEvent — thinking trace (glass-box M1)", () => {
  it("folds a disclosure into the live trace", () => {
    const s = useActivityStore.getState();
    s.ingestThinkingEvent(think({ eventId: "t1", actionId: "a1", sequence: 1 }));
    expect(useActivityStore.getState().thinking).toHaveLength(1);
    expect(selectRecentThinking(useActivityStore.getState())[0].eventId).toBe("t1");
  });

  it("ignores a duplicate eventId", () => {
    const s = useActivityStore.getState();
    s.ingestThinkingEvent(think({ eventId: "t1", actionId: "a1", sequence: 1 }));
    s.ingestThinkingEvent(think({ eventId: "t1", actionId: "a1", sequence: 1 }));
    expect(useActivityStore.getState().thinking).toHaveLength(1);
  });

  it("orders the trace by sequence regardless of arrival order", () => {
    const s = useActivityStore.getState();
    s.ingestThinkingEvent(think({ eventId: "t3", actionId: "a3", sequence: 3 }));
    s.ingestThinkingEvent(think({ eventId: "t1", actionId: "a1", sequence: 1 }));
    s.ingestThinkingEvent(think({ eventId: "t2", actionId: "a2", sequence: 2 }));
    expect(useActivityStore.getState().thinking.map((t) => t.sequence)).toEqual([1, 2, 3]);
  });

  it("exposes the newest disclosure first to the view", () => {
    const s = useActivityStore.getState();
    s.ingestThinkingEvent(think({ eventId: "t1", actionId: "a1", sequence: 1 }));
    s.ingestThinkingEvent(think({ eventId: "t2", actionId: "a2", sequence: 2 }));
    expect(selectRecentThinking(useActivityStore.getState()).map((t) => t.eventId)).toEqual([
      "t2",
      "t1",
    ]);
  });

  it("caps the trace at THINKING_CAP, keeping the highest sequences", () => {
    const s = useActivityStore.getState();
    for (let i = 1; i <= THINKING_CAP + 10; i++) {
      s.ingestThinkingEvent(think({ eventId: `t${i}`, actionId: `a${i}`, sequence: i }));
    }
    const { thinking, seenThinkingIds } = useActivityStore.getState();
    expect(thinking).toHaveLength(THINKING_CAP);
    expect(thinking[0].sequence).toBe(11);
    expect(thinking[thinking.length - 1].sequence).toBe(THINKING_CAP + 10);
    expect(seenThinkingIds.size).toBeLessThanOrEqual(THINKING_CAP);
  });

  it("carries a redacted plan + verifiable act, never a raw-CoT field", () => {
    // The shape itself enforces verifiable-action-first: a ThinkingEvent has no
    // free-text reasoning field. Assert the disclosure is exactly plan + tool /
    // source, and that the calibration label (koe-sua.2) stays unset in M1.
    const s = useActivityStore.getState();
    s.ingestThinkingEvent(think({ eventId: "t1", actionId: "a1", sequence: 1 }));
    const t = selectRecentThinking(useActivityStore.getState())[0];
    expect(t.plan).toBe("ウェブを検索しています");
    expect(t.tool).toBe("web_search");
    expect(t.source).toBe("web");
    expect(t.confidence).toBeUndefined();
  });

  it("keeps a disclosure visible through execution and clears it on completion", () => {
    // The disclosure stays up while the tool runs (a perceptible window to read it
    // and decide whether to intervene), then clears when the action COMPLETES — not
    // the instant it starts, which would flicker for ~0ms since the backend emits
    // the disclosure immediately before dispatch (cr R-B.5).
    const s = useActivityStore.getState();
    s.ingestThinkingEvent(think({ eventId: "t1", actionId: "shared", sequence: 1 }));
    const before = selectRecentThinking(useActivityStore.getState());
    expect(before).toHaveLength(1);
    expect(before[0].sequence).toBeLessThan(2); // minted below the start it precedes
    s.ingestToolEvent(ev({ eventId: "e1", actionId: "shared", sequence: 2, phase: "start" }));
    // Still shown, beside the live action, while it runs.
    expect(selectRecentThinking(useActivityStore.getState())).toHaveLength(1);
    s.ingestToolEvent(ev({ eventId: "e2", actionId: "shared", sequence: 3, phase: "done" }));
    // Cleared once the action completes.
    expect(selectRecentThinking(useActivityStore.getState())).toHaveLength(0);
    expect(useActivityStore.getState().actions.get("shared")?.actionId).toBe("shared");
  });

  it("keeps OTHER actions' disclosures when one action completes", () => {
    const s = useActivityStore.getState();
    s.ingestThinkingEvent(think({ eventId: "t1", actionId: "a1", sequence: 1 }));
    s.ingestThinkingEvent(think({ eventId: "t2", actionId: "a2", sequence: 2 }));
    s.ingestToolEvent(ev({ eventId: "e1", actionId: "a1", sequence: 3, phase: "done" }));
    // Only a1's disclosure clears on completion; a2 is untouched.
    expect(selectRecentThinking(useActivityStore.getState()).map((t) => t.actionId)).toEqual([
      "a2",
    ]);
  });

  it("drops a stale disclosure for a COMPLETED action but allows one beside an ACTIVE action", () => {
    // The tool-event and thinking-event ride separate, unordered Tauri channels, so
    // a tool-event can reach the store before its thinking-event. If the action is
    // already terminal the disclosure is stale (drop it, Codex Cloud P2); if it is
    // merely active the disclosure is still accurate intent and rides alongside.
    const s = useActivityStore.getState();
    // Already completed → stale → dropped.
    s.ingestToolEvent(ev({ eventId: "e1", actionId: "done1", sequence: 2, phase: "done" }));
    s.ingestThinkingEvent(think({ eventId: "t1", actionId: "done1", sequence: 1 }));
    expect(
      selectRecentThinking(useActivityStore.getState()).some((t) => t.actionId === "done1"),
    ).toBe(false);
    // Active (started, not done) → accurate intent → shown beside the live action.
    s.ingestToolEvent(ev({ eventId: "e2", actionId: "live1", sequence: 3, phase: "start" }));
    s.ingestThinkingEvent(think({ eventId: "t2", actionId: "live1", sequence: 4 }));
    expect(
      selectRecentThinking(useActivityStore.getState()).some((t) => t.actionId === "live1"),
    ).toBe(true);
  });

  it("clears pending disclosures when the session stops (idle) or errors", () => {
    const s = useActivityStore.getState();
    s.ingestThinkingEvent(think({ eventId: "t1", actionId: "a1", sequence: 1 }));
    s.setSessionStatus({ state: "idle", sequence: 1 });
    expect(useActivityStore.getState().thinking).toHaveLength(0);
    expect(useActivityStore.getState().seenThinkingIds.size).toBe(0);
    // And on error.
    s.ingestThinkingEvent(think({ eventId: "t2", actionId: "a2", sequence: 2 }));
    s.setSessionStatus({ state: "error", error: "boom", sequence: 2 });
    expect(useActivityStore.getState().thinking).toHaveLength(0);
  });

  it("reset clears the thinking trace", () => {
    const s = useActivityStore.getState();
    s.ingestThinkingEvent(think({ eventId: "t1", actionId: "a1", sequence: 1 }));
    s.reset();
    expect(useActivityStore.getState().thinking).toHaveLength(0);
    expect(useActivityStore.getState().seenThinkingIds.size).toBe(0);
  });
});

describe("selectDisplayStatus — derived state machine", () => {
  let statusSeq = 0;
  const setConn = (state: Parameters<typeof selectDisplayStatus>[0]["connState"]) =>
    useActivityStore.getState().setSessionStatus({ state, sequence: ++statusSeq });

  beforeEach(() => {
    statusSeq = 0;
  });

  it("idle / connecting map directly", () => {
    setConn("idle");
    expect(selectDisplayStatus(useActivityStore.getState())).toBe("idle");
    setConn("connecting");
    expect(selectDisplayStatus(useActivityStore.getState())).toBe("connecting");
  });

  it("reconnecting maps to reconnecting, not idle/conversing (koe-byf)", () => {
    setConn("reconnecting");
    expect(selectDisplayStatus(useActivityStore.getState())).toBe("reconnecting");
    // It is NOT a terminal state: connState is set and no sticky error is created.
    expect(useActivityStore.getState().connState).toBe("reconnecting");
    expect(useActivityStore.getState().lastError).toBeNull();
  });

  it("connected with no active tool = conversing", () => {
    setConn("connected");
    expect(selectDisplayStatus(useActivityStore.getState())).toBe("conversing");
  });

  it("connected with an active tool = working", () => {
    setConn("connected");
    useActivityStore
      .getState()
      .ingestToolEvent(ev({ eventId: "e1", actionId: "a1", sequence: 1, phase: "start" }));
    expect(selectDisplayStatus(useActivityStore.getState())).toBe("working");
  });

  it("error is shown and is sticky until a newer connection state arrives", () => {
    useActivityStore.getState().setSessionStatus({ state: "error", error: "boom", sequence: 10 });
    expect(selectDisplayStatus(useActivityStore.getState())).toBe("error");
    expect(useActivityStore.getState().lastError).toBe("boom");
    useActivityStore.getState().setSessionStatus({ state: "connecting", sequence: 11 });
    expect(selectDisplayStatus(useActivityStore.getState())).toBe("connecting");
    expect(useActivityStore.getState().lastError).toBeNull();
  });

  it("ignores a stale status event (older sequence cannot clear a newer error)", () => {
    useActivityStore.getState().setSessionStatus({ state: "error", error: "down", sequence: 10 });
    // A late 'connected' with a LOWER sequence must not revert the error.
    useActivityStore.getState().setSessionStatus({ state: "connected", sequence: 9 });
    expect(selectDisplayStatus(useActivityStore.getState())).toBe("error");
    expect(useActivityStore.getState().lastError).toBe("down");
  });

  it("accepts a first status event with sequence 0 (0-based backend)", () => {
    useActivityStore.getState().setSessionStatus({ state: "connected", sequence: 0 });
    expect(useActivityStore.getState().connState).toBe("connected");
  });
});

describe("approval queue — FIFO", () => {
  it("enqueues and dequeues by approvalId in order", () => {
    const s = useActivityStore.getState();
    s.enqueueApproval(approval({ approvalId: "first" }));
    s.enqueueApproval(approval({ approvalId: "second" }));
    expect(useActivityStore.getState().approvalQueue.map((a) => a.approvalId)).toEqual([
      "first",
      "second",
    ]);
    s.dequeueApproval("first");
    expect(useActivityStore.getState().approvalQueue.map((a) => a.approvalId)).toEqual(["second"]);
  });

  it("ignores a duplicate approvalId", () => {
    const s = useActivityStore.getState();
    s.enqueueApproval(approval({ approvalId: "x" }));
    s.enqueueApproval(approval({ approvalId: "x" }));
    expect(useActivityStore.getState().approvalQueue).toHaveLength(1);
  });
});

describe("reset", () => {
  it("clears events, actions, approvals and status", () => {
    const s = useActivityStore.getState();
    s.ingestToolEvent(ev({ eventId: "e1", actionId: "a1", sequence: 1, phase: "start" }));
    s.enqueueApproval(approval({ approvalId: "a" }));
    s.setSessionStatus({ state: "connected", sequence: 1 });
    s.reset();
    const after = useActivityStore.getState();
    expect(after.events).toHaveLength(0);
    expect(after.actions.size).toBe(0);
    expect(after.approvalQueue).toHaveLength(0);
    expect(after.connState).toBe("idle");
    expect(after.lastError).toBeNull();
  });
});
