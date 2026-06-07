import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

// Capture the handlers the hook registers, and hand back unlisten spies.
const handlers: Record<string, ((p: unknown) => void) | undefined> = {};
const unlistenSpies: Array<ReturnType<typeof vi.fn>> = [];

function makeOn(key: string) {
  return (handler: (p: unknown) => void) => {
    handlers[key] = handler;
    const spy = vi.fn();
    unlistenSpies.push(spy);
    return Promise.resolve(spy);
  };
}

vi.mock("../../lib/tauri/ipc", () => ({
  onToolEvent: (h: (p: unknown) => void) => makeOn("tool")(h),
  onThinkingEvent: (h: (p: unknown) => void) => makeOn("thinking")(h),
  onApprovalRequired: (h: (p: unknown) => void) => makeOn("approval")(h),
  onSessionStatus: (h: (p: unknown) => void) => makeOn("status")(h),
}));

import { useActivityStore } from "./activityStore";
import { useActivityEvents } from "./useActivityEvents";
import type { ApprovalRequest, ThinkingEvent, ToolEvent } from "./types";

beforeEach(() => {
  for (const k of Object.keys(handlers)) delete handlers[k];
  unlistenSpies.length = 0;
  useActivityStore.getState().reset();
});

describe("useActivityEvents", () => {
  it("subscribes to all four channels", async () => {
    renderHook(() => useActivityEvents());
    await waitFor(() => {
      expect(handlers.tool).toBeTypeOf("function");
      expect(handlers.thinking).toBeTypeOf("function");
      expect(handlers.approval).toBeTypeOf("function");
      expect(handlers.status).toBeTypeOf("function");
    });
  });

  it("routes thinking events into the store", async () => {
    renderHook(() => useActivityEvents());
    await waitFor(() => expect(handlers.thinking).toBeTypeOf("function"));
    const event: ThinkingEvent = {
      eventId: "t1",
      actionId: "a1",
      sequence: 1,
      phase: "deciding",
      plan: "ウェブを検索しています",
      tool: "web_search",
      source: "web",
      timestamp: 1000,
    };
    act(() => handlers.thinking?.(event));
    expect(useActivityStore.getState().thinking).toHaveLength(1);
  });

  it("routes tool events into the store", async () => {
    renderHook(() => useActivityEvents());
    await waitFor(() => expect(handlers.tool).toBeTypeOf("function"));
    const event: ToolEvent = {
      eventId: "e1",
      actionId: "a1",
      sequence: 1,
      tool: "web_search",
      phase: "start",
      timestamp: 1000,
      displaySummary: "searching",
    };
    act(() => handlers.tool?.(event));
    expect(useActivityStore.getState().events).toHaveLength(1);
  });

  it("routes approval requests into the queue", async () => {
    renderHook(() => useActivityEvents());
    await waitFor(() => expect(handlers.approval).toBeTypeOf("function"));
    const req: ApprovalRequest = {
      approvalId: "ap1",
      tool: "run_command",
      risk: "DANGER",
      displaySummary: "rm something",
      deadlineAt: 30_000,
      sequence: 1,
    };
    act(() => handlers.approval?.(req));
    expect(useActivityStore.getState().approvalQueue).toHaveLength(1);
  });

  it("unlistens every channel on unmount", async () => {
    const { unmount } = renderHook(() => useActivityEvents());
    await waitFor(() => expect(unlistenSpies).toHaveLength(4));
    unmount();
    await waitFor(() => {
      for (const spy of unlistenSpies) expect(spy).toHaveBeenCalledTimes(1);
    });
  });
});
