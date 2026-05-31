// Tests for useSessionEvents: verifies that the hook subscribes to the
// session-status channel and routes events into sessionStore.

import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

// ---------------------------------------------------------------------------
// Mock IPC — capture the handler registered by the hook.
// ---------------------------------------------------------------------------

// vi.hoisted ensures these are evaluated before the module factory runs
// (vi.mock factories are hoisted to the top of the file by Vitest/esbuild).
const { mockStopSession, mockStartSession } = vi.hoisted(() => ({
  mockStopSession: vi.fn<[], Promise<void>>(),
  mockStartSession: vi.fn<[], Promise<void>>(),
}));

let capturedHandler: ((p: unknown) => void) | undefined;
const unlistenSpy = vi.fn();
// Controls whether the next onSessionStatus call resolves or rejects.
let ipcShouldReject = false;

vi.mock("../../lib/tauri/ipc", () => ({
  onSessionStatus: (handler: (p: unknown) => void) => {
    capturedHandler = handler;
    if (ipcShouldReject) {
      return Promise.reject(new Error("listen() failed"));
    }
    return Promise.resolve(unlistenSpy);
  },
  stopSession: mockStopSession,
  startSession: mockStartSession,
}));

import { useSessionStore } from "./sessionStore";
import { useSessionEvents } from "./useSessionEvents";
import type { SessionStatusEvent } from "../activity/types";

beforeEach(() => {
  capturedHandler = undefined;
  unlistenSpy.mockReset();
  mockStopSession.mockReset();
  mockStartSession.mockReset();
  ipcShouldReject = false;
  useSessionStore.getState().reset();
});

describe("useSessionEvents", () => {
  it("subscribes to the session-status channel on mount", async () => {
    renderHook(() => useSessionEvents());
    await waitFor(() => expect(capturedHandler).toBeTypeOf("function"));
  });

  it("routes a connected event into sessionStore", async () => {
    renderHook(() => useSessionEvents());
    await waitFor(() => expect(capturedHandler).toBeTypeOf("function"));

    const event: SessionStatusEvent = { state: "connected", sequence: 1 };
    act(() => capturedHandler?.(event));

    expect(useSessionStore.getState().status).toBe("connected");
  });

  it("routes an error event (with message) into sessionStore", async () => {
    renderHook(() => useSessionEvents());
    await waitFor(() => expect(capturedHandler).toBeTypeOf("function"));

    const event: SessionStatusEvent = { state: "error", sequence: 1, error: "net fail" };
    act(() => capturedHandler?.(event));

    expect(useSessionStore.getState().status).toBe("error");
    expect(useSessionStore.getState().error).toBe("net fail");
  });

  it("calls the unlisten function when the hook unmounts", async () => {
    // Wait for the async onSessionStatus promise to resolve before unmounting,
    // so the cleanup-function path (unlisten?.()) is exercised — not the
    // early-unmount branch (else { fn(); }).
    const { unmount } = renderHook(() => useSessionEvents());
    await waitFor(() => expect(capturedHandler).toBeTypeOf("function"));
    unmount();
    expect(unlistenSpy).toHaveBeenCalledTimes(1);
  });

  it("surfaces listen() rejection to sessionStore as an error state", async () => {
    // P1: swallowed .catch must now call setListenerError so the store moves
    // to 'error' — preventing the UI from being stuck on 'idle'/'loading'
    // with an unwired session-status channel.
    ipcShouldReject = true;
    renderHook(() => useSessionEvents());
    await waitFor(() =>
      expect(useSessionStore.getState().status).toBe("error"),
    );
    expect(useSessionStore.getState().listenerFailed).toBe(true);
    expect(useSessionStore.getState().error).not.toBeNull();
  });

  it("does not call setListenerError when the hook unmounts before listen() rejects", async () => {
    // If !active (unmounted before the promise settled), the store must NOT be
    // touched — the component that would show the error is already gone.
    ipcShouldReject = true;
    const { unmount } = renderHook(() => useSessionEvents());
    // Unmount synchronously before the microtask queue drains.
    unmount();
    // Let the rejected promise settle.
    await new Promise<void>((r) => setTimeout(r, 0));
    expect(useSessionStore.getState().status).toBe("idle");
    expect(useSessionStore.getState().listenerFailed).toBe(false);
  });

  // (b) if listen rejects WHILE a session is running, stopSession is called
  it("(P1-b) fires stopSession when listen() rejects during a running session (connected)", async () => {
    // Pre-condition: session is connected (started before listener failure).
    act(() => {
      useSessionStore.getState().setFromEvent({ state: "connected", sequence: 1 });
    });
    expect(useSessionStore.getState().status).toBe("connected");

    mockStopSession.mockResolvedValueOnce(undefined);
    ipcShouldReject = true;
    renderHook(() => useSessionEvents());

    // Wait for the listener failure to propagate and the stop to be called.
    await waitFor(() =>
      expect(useSessionStore.getState().listenerFailed).toBe(true),
    );
    await waitFor(() => expect(mockStopSession).toHaveBeenCalledTimes(1));
  });

  it("(P1-b) fires stopSession when listen() rejects during a loading session", async () => {
    // Pre-condition: session is in 'loading' (started but not yet connected).
    act(() => {
      useSessionStore.getState().setFromEvent({ state: "connecting", sequence: 1 });
    });
    expect(useSessionStore.getState().status).toBe("loading");

    mockStopSession.mockResolvedValueOnce(undefined);
    ipcShouldReject = true;
    renderHook(() => useSessionEvents());

    await waitFor(() =>
      expect(useSessionStore.getState().listenerFailed).toBe(true),
    );
    await waitFor(() => expect(mockStopSession).toHaveBeenCalledTimes(1));
  });

  it("(P1-b) does NOT fire stopSession when listen() rejects in idle state (no orphan)", async () => {
    // No session running — no backend to stop.
    expect(useSessionStore.getState().status).toBe("idle");

    mockStopSession.mockResolvedValueOnce(undefined);
    ipcShouldReject = true;
    renderHook(() => useSessionEvents());

    await waitFor(() =>
      expect(useSessionStore.getState().listenerFailed).toBe(true),
    );
    // stopSession should NOT have been called (no running session to orphan).
    expect(mockStopSession).not.toHaveBeenCalled();
  });
});
