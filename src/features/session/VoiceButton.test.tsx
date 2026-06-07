// Render tests for VoiceButton: verifies accessible labelling, toggle
// behaviour, loading / error state rendering, and that it reads from
// sessionStore.

import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// ---------------------------------------------------------------------------
// Mock IPC so no Tauri runtime is needed.
// ---------------------------------------------------------------------------

const mockStartSession = vi.hoisted(() => vi.fn<[], Promise<void>>());
const mockStopSession = vi.hoisted(() => vi.fn<[], Promise<void>>());

vi.mock("../../lib/tauri/ipc", () => ({
  startSession: mockStartSession,
  stopSession: mockStopSession,
}));

import { VoiceButton } from "./VoiceButton";
import { useSessionStore } from "./sessionStore";

beforeEach(() => {
  useSessionStore.getState().reset();
  vi.clearAllMocks();
});

afterEach(() => {
  useSessionStore.getState().reset();
});

describe("VoiceButton", () => {
  it("renders in idle state with start label", () => {
    render(<VoiceButton />);
    const btn = screen.getByRole("button");
    expect(btn).toBeInTheDocument();
    expect(btn).not.toBeDisabled();
    expect(btn.getAttribute("aria-label")).toMatch(/開始|start/i);
  });

  it("is accessible: has a role=button and a non-empty aria-label", () => {
    render(<VoiceButton />);
    const btn = screen.getByRole("button");
    const label = btn.getAttribute("aria-label") ?? btn.textContent ?? "";
    expect(label.trim().length).toBeGreaterThan(0);
  });

  it("calls startSession when clicked in idle state", async () => {
    mockStartSession.mockResolvedValueOnce(undefined);
    render(<VoiceButton />);
    const btn = screen.getByRole("button");
    await act(async () => {
      fireEvent.click(btn);
    });
    expect(mockStartSession).toHaveBeenCalledTimes(1);
  });

  it("calls stopSession when clicked in connected state", async () => {
    act(() => {
      useSessionStore.getState().setFromEvent({ state: "connected", sequence: 1 });
    });
    mockStopSession.mockResolvedValueOnce(undefined);
    render(<VoiceButton />);
    const btn = screen.getByRole("button");
    await act(async () => {
      fireEvent.click(btn);
    });
    expect(mockStopSession).toHaveBeenCalledTimes(1);
  });

  it("is disabled and shows a loading indicator while loading", () => {
    act(() => {
      useSessionStore.getState().setFromEvent({ state: "connecting", sequence: 1 });
    });
    render(<VoiceButton />);
    const btn = screen.getByRole("button");
    expect(btn).toBeDisabled();
    const isBusy =
      btn.getAttribute("aria-busy") === "true" ||
      /準備|connecting|…/i.test(btn.textContent ?? "");
    expect(isBusy).toBe(true);
  });

  it("stays enabled, busy, and stoppable while reconnecting (koe-byf)", async () => {
    act(() => {
      useSessionStore.getState().setFromEvent({ state: "reconnecting", sequence: 1 });
    });
    mockStopSession.mockResolvedValueOnce(undefined);
    render(<VoiceButton />);
    const btn = screen.getByRole("button");
    // Unlike `loading`, a reconnecting session must NOT trap the user — the button
    // is enabled, marked busy, and acts as "stop".
    expect(btn).not.toBeDisabled();
    expect(btn.getAttribute("aria-busy")).toBe("true");
    expect(btn.getAttribute("aria-pressed")).toBe("true");
    await act(async () => {
      fireEvent.click(btn);
    });
    expect(mockStopSession).toHaveBeenCalledTimes(1);
    expect(mockStartSession).not.toHaveBeenCalled();
  });

  it("shows an error alert when in error state", () => {
    act(() => {
      useSessionStore.getState().setFromEvent({
        state: "error",
        sequence: 1,
        error: "接続に失敗しました",
      });
    });
    render(<VoiceButton />);
    // Error message is surfaced as role=alert.
    expect(screen.getByRole("alert")).toBeInTheDocument();
  });

  it("keeps the button enabled in error state so the user can retry", () => {
    act(() => {
      useSessionStore.getState().setFromEvent({
        state: "error",
        sequence: 1,
        error: "timeout",
      });
    });
    render(<VoiceButton />);
    expect(screen.getByRole("button")).not.toBeDisabled();
  });

  it("reflects store state reactively: idle → connected", () => {
    render(<VoiceButton />);
    const btn = screen.getByRole("button");
    // Initially idle — button should reference starting.
    expect(btn.getAttribute("aria-label")).toMatch(/開始|start/i);

    // Transition to connected.
    act(() => {
      useSessionStore.getState().setFromEvent({ state: "connected", sequence: 1 });
    });
    // Now it should reference stopping.
    expect(btn.getAttribute("aria-label")).toMatch(/停止|stop/i);
  });

  describe("listenerFailed state (P1 fail-closed)", () => {
    function setListenerFailed() {
      act(() => {
        useSessionStore.getState().setListenerError();
      });
    }

    // (c) button is not permanently disabled in listener-failed state
    it("(P1-c) button is NOT disabled when listenerFailed=true", () => {
      setListenerFailed();
      render(<VoiceButton />);
      expect(screen.getByRole("button")).not.toBeDisabled();
    });

    it("(P1-c) button shows an actionable label (停止) when listenerFailed=true", () => {
      setListenerFailed();
      render(<VoiceButton />);
      const btn = screen.getByRole("button");
      // The label must reference stopping (not "再試行" / "話す" which imply start)
      expect(btn.textContent ?? btn.getAttribute("aria-label")).toMatch(/停止/);
    });

    // (a) clicking the button in listener-failed state does NOT call start_session
    it("(P1-a) clicking button in listenerFailed state does NOT call startSession", async () => {
      setListenerFailed();
      render(<VoiceButton />);
      const btn = screen.getByRole("button");
      await act(async () => {
        fireEvent.click(btn);
      });
      expect(mockStartSession).not.toHaveBeenCalled();
    });

    // (b) clicking the button in listener-failed state calls stopSession
    it("(P1-b) clicking button in listenerFailed state calls stopSession", async () => {
      setListenerFailed();
      mockStopSession.mockResolvedValueOnce(undefined);
      render(<VoiceButton />);
      const btn = screen.getByRole("button");
      await act(async () => {
        fireEvent.click(btn);
      });
      expect(mockStopSession).toHaveBeenCalledTimes(1);
    });

    it("shows the listener error message as role=alert", () => {
      setListenerFailed();
      render(<VoiceButton />);
      expect(screen.getByRole("alert")).toBeInTheDocument();
    });
  });

  describe("aria-pressed (toggle semantics)", () => {
    it("is false in idle state", () => {
      render(<VoiceButton />);
      const btn = screen.getByRole("button");
      expect(btn.getAttribute("aria-pressed")).toBe("false");
    });

    it("is true in connected state", () => {
      act(() => {
        useSessionStore.getState().setFromEvent({ state: "connected", sequence: 1 });
      });
      render(<VoiceButton />);
      const btn = screen.getByRole("button");
      expect(btn.getAttribute("aria-pressed")).toBe("true");
    });

    it("is false in loading state", () => {
      act(() => {
        useSessionStore.getState().setFromEvent({ state: "connecting", sequence: 1 });
      });
      render(<VoiceButton />);
      const btn = screen.getByRole("button");
      expect(btn.getAttribute("aria-pressed")).toBe("false");
    });

    it("is false in error state", () => {
      act(() => {
        useSessionStore
          .getState()
          .setFromEvent({ state: "error", sequence: 1, error: "timeout" });
      });
      render(<VoiceButton />);
      const btn = screen.getByRole("button");
      expect(btn.getAttribute("aria-pressed")).toBe("false");
    });
  });
});
