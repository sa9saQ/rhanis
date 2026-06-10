// Tests for sessionStore: state transitions, IPC calls, event-driven updates.
//
// IPC is mocked at the module boundary so tests run without a Tauri runtime.
// `vi.mock` with `vi.hoisted` hoists factories above ESM import evaluation.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act } from "@testing-library/react";

// ---------------------------------------------------------------------------
// Module-level mock setup (must be before imports that use the module).
// ---------------------------------------------------------------------------

const mockStartSession = vi.hoisted(() => vi.fn<[], Promise<void>>());
const mockStopSession = vi.hoisted(() => vi.fn<[], Promise<void>>());

vi.mock("../../lib/tauri/ipc", () => ({
  startSession: mockStartSession,
  stopSession: mockStopSession,
}));

// Import the store AFTER mocks are declared.
import { useSessionStore } from "./sessionStore";
import type { SessionStatusEvent } from "../activity/types";

function statusEvent(
  state: SessionStatusEvent["state"],
  sequence: number,
  error?: string,
): SessionStatusEvent {
  return { state, sequence, ...(error ? { error } : {}) };
}

beforeEach(() => {
  useSessionStore.getState().reset();
  vi.clearAllMocks();
});

afterEach(() => {
  useSessionStore.getState().reset();
});

// ---------------------------------------------------------------------------
// Initial state
// ---------------------------------------------------------------------------

describe("initial state", () => {
  it("starts idle with no error", () => {
    const s = useSessionStore.getState();
    expect(s.status).toBe("idle");
    expect(s.error).toBeNull();
  });
});

// ---------------------------------------------------------------------------
// startSession
// ---------------------------------------------------------------------------

describe("startSession", () => {
  it("transitions to loading while ipc call is in-flight", async () => {
    let resolve!: () => void;
    mockStartSession.mockReturnValueOnce(new Promise<void>((r) => (resolve = r)));

    const promise = useSessionStore.getState().startSession();
    expect(useSessionStore.getState().status).toBe("loading");
    expect(useSessionStore.getState().error).toBeNull();

    resolve();
    await promise;
  });

  it("calls ipcStartSession exactly once", async () => {
    mockStartSession.mockResolvedValueOnce(undefined);
    await useSessionStore.getState().startSession();
    expect(mockStartSession).toHaveBeenCalledTimes(1);
  });

  it("sets error status when ipc rejects", async () => {
    mockStartSession.mockRejectedValueOnce(new Error("budget exceeded"));
    await useSessionStore.getState().startSession();
    expect(useSessionStore.getState().status).toBe("error");
    expect(useSessionStore.getState().error).not.toBeNull();
  });

  it("preserves a backend-emitted error reason if the invoke also rejects (race fix)", async () => {
    // P2 race: the backend emits session-status {state:'error', error:'budget exceeded'}
    // before the invoke() promise rejects.  The catch block must not overwrite
    // the specific reason with the generic fallback message.
    const specificReason = "月次予算の上限に達しました";

    // Arrange: a start call that will reject…
    let rejectStart!: (e: unknown) => void;
    mockStartSession.mockReturnValueOnce(
      new Promise<void>((_, r) => (rejectStart = r)),
    );

    const promise = useSessionStore.getState().startSession();
    // Store is now in 'loading'.  Simulate the backend emitting error status
    // before the invoke rejects (= the race window).
    act(() => {
      useSessionStore
        .getState()
        .setFromEvent(statusEvent("error", 1, specificReason));
    });
    expect(useSessionStore.getState().status).toBe("error");
    expect(useSessionStore.getState().error).toBe(specificReason);

    // Now the invoke rejects — the catch must not clobber the specific reason.
    rejectStart(new Error("reject"));
    await promise;

    expect(useSessionStore.getState().status).toBe("error");
    expect(useSessionStore.getState().error).toBe(specificReason);
  });

  it("is a no-op when already in loading state", async () => {
    // First call hangs — puts store in loading.
    let resolve!: () => void;
    mockStartSession.mockReturnValueOnce(new Promise<void>((r) => (resolve = r)));
    const first = useSessionStore.getState().startSession();

    // Second call while loading: should be a no-op.
    await useSessionStore.getState().startSession();
    expect(mockStartSession).toHaveBeenCalledTimes(1);

    resolve();
    await first;
  });

  it("is a no-op when already connected", async () => {
    mockStartSession.mockResolvedValueOnce(undefined);
    await useSessionStore.getState().startSession();
    // Drive to connected via event.
    act(() => {
      useSessionStore.getState().setFromEvent(statusEvent("connected", 1));
    });
    expect(useSessionStore.getState().status).toBe("connected");

    await useSessionStore.getState().startSession();
    expect(mockStartSession).toHaveBeenCalledTimes(1);
  });

  it("is a no-op when reconnecting (koe-byf: already an active session)", async () => {
    act(() => {
      useSessionStore.getState().setFromEvent(statusEvent("reconnecting", 1));
    });
    expect(useSessionStore.getState().status).toBe("reconnecting");
    await useSessionStore.getState().startSession();
    expect(mockStartSession).not.toHaveBeenCalled();
  });

  it("clears a previous error on new start", async () => {
    // Plant an error.
    act(() => {
      useSessionStore.getState().setFromEvent(statusEvent("error", 1, "prev error"));
    });
    expect(useSessionStore.getState().error).not.toBeNull();

    // Start again.
    let resolve!: () => void;
    mockStartSession.mockReturnValueOnce(new Promise<void>((r) => (resolve = r)));
    const p = useSessionStore.getState().startSession();
    expect(useSessionStore.getState().error).toBeNull();
    resolve();
    await p;
  });
});

// ---------------------------------------------------------------------------
// stopSession
// ---------------------------------------------------------------------------

describe("stopSession", () => {
  async function connectStore() {
    mockStartSession.mockResolvedValueOnce(undefined);
    await useSessionStore.getState().startSession();
    act(() => {
      useSessionStore.getState().setFromEvent(statusEvent("connected", 1));
    });
  }

  it("calls ipcStopSession when connected", async () => {
    await connectStore();
    mockStopSession.mockResolvedValueOnce(undefined);
    await useSessionStore.getState().stopSession();
    expect(mockStopSession).toHaveBeenCalledTimes(1);
  });

  it("is a no-op when idle", async () => {
    await useSessionStore.getState().stopSession();
    expect(mockStopSession).not.toHaveBeenCalled();
  });

  it("calls ipcStopSession when reconnecting (koe-byf: reconnecting is stoppable)", async () => {
    act(() => {
      useSessionStore.getState().setFromEvent(statusEvent("reconnecting", 1));
    });
    expect(useSessionStore.getState().status).toBe("reconnecting");
    mockStopSession.mockResolvedValueOnce(undefined);
    await useSessionStore.getState().stopSession();
    expect(mockStopSession).toHaveBeenCalledTimes(1);
  });

  it("calls ipcStopSession in loading state (koe-5fs: escape hatch for a hung 準備中)", async () => {
    // Drive the store to 'loading' via a backend event (the case where the IPC
    // response arrives before the first event). The old guard made stop a no-op
    // here; koe-5fs makes "loading" stoppable — run_session_supervised races
    // connect() against the master stop (session_manager.rs ~1095), so a stop
    // mid-connect cleanly abandons the attempt instead of orphaning it, giving
    // the user an escape from a hung connect (symptom 4).
    act(() => {
      useSessionStore.getState().setFromEvent(statusEvent("connecting", 1));
    });
    expect(useSessionStore.getState().status).toBe("loading");
    mockStopSession.mockResolvedValueOnce(undefined);
    await useSessionStore.getState().stopSession();
    expect(mockStopSession).toHaveBeenCalledTimes(1);
  });

  it("double-stop guard still holds in loading (concurrent stop is a no-op)", async () => {
    // The stopInFlight guard is state-agnostic, so making loading stoppable must
    // not open a double-stop window: a second concurrent stop still bails.
    act(() => {
      useSessionStore.getState().setFromEvent(statusEvent("connecting", 1));
    });
    let release: (() => void) | undefined;
    mockStopSession.mockImplementationOnce(
      () =>
        new Promise<void>((resolve) => {
          release = resolve;
        }),
    );
    const first = useSessionStore.getState().stopSession();
    // Second call while the first is still in flight: must not invoke ipc again.
    await useSessionStore.getState().stopSession();
    expect(mockStopSession).toHaveBeenCalledTimes(1);
    release?.();
    await first;
    expect(mockStopSession).toHaveBeenCalledTimes(1);
  });

  it("is a no-op in error state (backend session already cleared)", async () => {
    act(() => {
      useSessionStore.getState().setFromEvent(statusEvent("error", 1, "budget exceeded"));
    });
    expect(useSessionStore.getState().status).toBe("error");
    await useSessionStore.getState().stopSession();
    expect(mockStopSession).not.toHaveBeenCalled();
  });

  it("sets error if ipc rejects", async () => {
    await connectStore();
    mockStopSession.mockRejectedValueOnce(new Error("stop failed"));
    await useSessionStore.getState().stopSession();
    expect(useSessionStore.getState().status).toBe("error");
  });
});

// ---------------------------------------------------------------------------
// setFromEvent (event-driven status updates)
// ---------------------------------------------------------------------------

describe("setFromEvent", () => {
  it("transitions idle → loading → connected via events", () => {
    const s = useSessionStore.getState();
    act(() => s.setFromEvent(statusEvent("connecting", 1)));
    expect(useSessionStore.getState().status).toBe("loading");

    act(() => useSessionStore.getState().setFromEvent(statusEvent("connected", 2)));
    expect(useSessionStore.getState().status).toBe("connected");
  });

  it("transitions connected → reconnecting → connected (koe-byf)", () => {
    act(() => useSessionStore.getState().setFromEvent(statusEvent("connected", 1)));
    expect(useSessionStore.getState().status).toBe("connected");

    act(() => useSessionStore.getState().setFromEvent(statusEvent("reconnecting", 2)));
    expect(useSessionStore.getState().status).toBe("reconnecting");
    // A reconnect carries no error (it is recovering, not failed).
    expect(useSessionStore.getState().error).toBeNull();

    act(() => useSessionStore.getState().setFromEvent(statusEvent("connected", 3)));
    expect(useSessionStore.getState().status).toBe("connected");
  });

  it("captures the error message on error state", () => {
    act(() => {
      useSessionStore.getState().setFromEvent(statusEvent("error", 1, "timeout"));
    });
    expect(useSessionStore.getState().status).toBe("error");
    expect(useSessionStore.getState().error).toBe("timeout");
  });

  it("uses a fallback message when error has no message", () => {
    act(() => {
      useSessionStore.getState().setFromEvent(statusEvent("error", 1));
    });
    expect(useSessionStore.getState().error).not.toBeNull();
  });

  it("ignores a stale event (sequence <= lastSequence)", () => {
    act(() => {
      useSessionStore.getState().setFromEvent(statusEvent("connected", 5));
    });
    expect(useSessionStore.getState().status).toBe("connected");

    // A late "idle" with sequence 3 must not override.
    act(() => {
      useSessionStore.getState().setFromEvent(statusEvent("idle", 3));
    });
    expect(useSessionStore.getState().status).toBe("connected");
  });

  it("clears error when a non-error status arrives with a newer sequence", () => {
    act(() => {
      useSessionStore.getState().setFromEvent(statusEvent("error", 1, "net fail"));
    });
    act(() => {
      useSessionStore.getState().setFromEvent(statusEvent("connected", 2));
    });
    expect(useSessionStore.getState().status).toBe("connected");
    expect(useSessionStore.getState().error).toBeNull();
  });
});

// ---------------------------------------------------------------------------
// listenerFailed / P1 fail-closed guards
// ---------------------------------------------------------------------------

describe("listenerFailed (P1 fail-closed)", () => {
  it("setListenerError sets listenerFailed=true and status='error'", () => {
    useSessionStore.getState().setListenerError();
    const s = useSessionStore.getState();
    expect(s.listenerFailed).toBe(true);
    expect(s.status).toBe("error");
    expect(s.error).not.toBeNull();
  });

  // (a) after listen reject, startSession does NOT call invoke('start_session')
  it("(P1-a) startSession is a no-op (does not call ipc) when listenerFailed=true", async () => {
    useSessionStore.getState().setListenerError();
    expect(useSessionStore.getState().listenerFailed).toBe(true);

    await useSessionStore.getState().startSession();

    expect(mockStartSession).not.toHaveBeenCalled();
    // Still in error state, error message updated to mention restart
    expect(useSessionStore.getState().status).toBe("error");
    expect(useSessionStore.getState().listenerFailed).toBe(true);
  });

  // (b) stopSession is reachable when listenerFailed (no orphaned session)
  it("(P1-b) stopSession is callable (not a no-op) when listenerFailed=true", async () => {
    // Simulate: session was connected, then listener failed
    mockStartSession.mockResolvedValueOnce(undefined);
    await useSessionStore.getState().startSession();
    act(() => {
      useSessionStore.getState().setFromEvent(statusEvent("connected", 1));
    });
    expect(useSessionStore.getState().status).toBe("connected");

    // Listener fails while session is running
    useSessionStore.getState().setListenerError();
    expect(useSessionStore.getState().listenerFailed).toBe(true);
    expect(useSessionStore.getState().status).toBe("error");

    // stopSession must now reach ipc (not skip because of error state)
    mockStopSession.mockResolvedValueOnce(undefined);
    await useSessionStore.getState().stopSession();
    expect(mockStopSession).toHaveBeenCalledTimes(1);
  });

  it("(P1-b) stopSession is still a no-op for normal (non-listener) error state", async () => {
    act(() => {
      useSessionStore.getState().setFromEvent(statusEvent("error", 1, "timeout"));
    });
    expect(useSessionStore.getState().listenerFailed).toBe(false);
    await useSessionStore.getState().stopSession();
    expect(mockStopSession).not.toHaveBeenCalled();
  });

  // (a) THE KEY REGRESSION: listen() rejects WHILE ipcStartSession is in-flight
  // (startInFlight=true). The forced-stop path must bypass the start in-flight
  // guard and still reach ipcStopSession(). Previously a shared `inFlight` guard
  // blocked this path, leaving an orphaned, cost-accumulating backend session.
  it("(P1-a) ipcStopSession IS called when listen() rejects while ipcStartSession is pending", async () => {
    // Arrange: a start call that hangs (startInFlight=true while we test).
    let resolveStart!: () => void;
    mockStartSession.mockReturnValueOnce(
      new Promise<void>((r) => (resolveStart = r)),
    );
    // Fire startSession but don't await — it's in-flight now.
    const startPromise = useSessionStore.getState().startSession();
    expect(useSessionStore.getState().status).toBe("loading");

    // Act: simulate what useSessionEvents does when listen() rejects in this window.
    // 1. setListenerError() — sets status="error", listenerFailed=true.
    // 2. stopSession() — must reach ipcStopSession() despite startInFlight=true.
    mockStopSession.mockResolvedValueOnce(undefined);
    act(() => {
      useSessionStore.getState().setListenerError();
    });
    expect(useSessionStore.getState().listenerFailed).toBe(true);
    expect(useSessionStore.getState().status).toBe("error");

    await useSessionStore.getState().stopSession();

    // Assert: ipcStopSession was called (the backend was told to stop).
    expect(mockStopSession).toHaveBeenCalledTimes(1);

    // Clean up: let the in-flight start settle so no dangling promises.
    resolveStart();
    await startPromise;
  });

  // (b) Double-stop prevention: stopSession called twice concurrently should
  // only invoke ipcStopSession once (stopInFlight guard).
  it("(P1-b-double) no double-stop when stopSession called twice concurrently", async () => {
    // Get to connected state.
    mockStartSession.mockResolvedValueOnce(undefined);
    await useSessionStore.getState().startSession();
    act(() => {
      useSessionStore.getState().setFromEvent(statusEvent("connected", 1));
    });
    expect(useSessionStore.getState().status).toBe("connected");

    // Arrange: first stop hangs.
    let resolveStop!: () => void;
    mockStopSession.mockReturnValueOnce(
      new Promise<void>((r) => (resolveStop = r)),
    );

    // Fire two concurrent stop calls.
    const stop1 = useSessionStore.getState().stopSession();
    const stop2 = useSessionStore.getState().stopSession(); // second while first in-flight

    resolveStop();
    await Promise.all([stop1, stop2]);

    // ipcStopSession must have been called exactly once.
    expect(mockStopSession).toHaveBeenCalledTimes(1);
  });
});

// ---------------------------------------------------------------------------
// reset
// ---------------------------------------------------------------------------

describe("reset", () => {
  it("restores idle state and clears error", () => {
    act(() => {
      useSessionStore.getState().setFromEvent(statusEvent("error", 1, "err"));
    });
    useSessionStore.getState().reset();
    const s = useSessionStore.getState();
    expect(s.status).toBe("idle");
    expect(s.error).toBeNull();
  });
});
