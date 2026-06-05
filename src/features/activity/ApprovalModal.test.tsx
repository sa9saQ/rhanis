import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const resolveToolApproval = vi.fn();
vi.mock("../../lib/tauri/ipc", () => ({
  resolveToolApproval: (...args: unknown[]) => resolveToolApproval(...args),
}));

import { ApprovalModal } from "./ApprovalModal";
import { useActivityStore } from "./activityStore";
import type { ApprovalRequest } from "./types";

function approval(partial: Partial<ApprovalRequest> & { approvalId: string }): ApprovalRequest {
  return {
    tool: "run_command",
    risk: "DANGER",
    displaySummary: "delete a file",
    deadlineAt: Date.now() + 30_000,
    sequence: 1,
    ...partial,
  };
}

beforeEach(() => {
  resolveToolApproval.mockReset();
  resolveToolApproval.mockResolvedValue(undefined);
  useActivityStore.getState().reset();
});

afterEach(() => {
  vi.useRealTimers();
});

describe("ApprovalModal", () => {
  it("renders nothing when the queue is empty", () => {
    const { container } = render(<ApprovalModal />);
    expect(container).toBeEmptyDOMElement();
  });

  it("shows the head request with its summary and risk", () => {
    render(<ApprovalModal />);
    act(() => useActivityStore.getState().enqueueApproval(approval({ approvalId: "a1" })));
    expect(screen.getByText("delete a file")).toBeInTheDocument();
    expect(screen.getByText("DANGER")).toBeInTheDocument();
  });

  it("approve resolves with the matching approvalId and dequeues", async () => {
    render(<ApprovalModal />);
    act(() => useActivityStore.getState().enqueueApproval(approval({ approvalId: "a1" })));
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: /許可/ }));
    });
    expect(resolveToolApproval).toHaveBeenCalledWith("a1", "approve");
    expect(useActivityStore.getState().approvalQueue).toHaveLength(0);
  });

  it("deny resolves with deny and dequeues", async () => {
    render(<ApprovalModal />);
    act(() => useActivityStore.getState().enqueueApproval(approval({ approvalId: "a1" })));
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: /拒否/ }));
    });
    expect(resolveToolApproval).toHaveBeenCalledWith("a1", "deny");
    expect(useActivityStore.getState().approvalQueue).toHaveLength(0);
  });

  it("shows how many more approvals are waiting", () => {
    render(<ApprovalModal />);
    act(() => {
      useActivityStore.getState().enqueueApproval(approval({ approvalId: "a1" }));
      useActivityStore.getState().enqueueApproval(approval({ approvalId: "a2" }));
    });
    expect(screen.getByText(/他に 1 件/)).toBeInTheDocument();
  });

  it("keeps the modal open and shows a fixed (non-leaking) error when the IPC fails", async () => {
    // The raw backend error could carry a path/key/PII — it must NOT be shown.
    resolveToolApproval.mockRejectedValueOnce(new Error("/secret/path leaked sk-123"));
    render(<ApprovalModal />);
    act(() => useActivityStore.getState().enqueueApproval(approval({ approvalId: "a1" })));
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: /許可/ }));
    });
    const alert = screen.getByRole("alert");
    expect(alert).toHaveTextContent("承認の送信に失敗しました");
    expect(alert).not.toHaveTextContent("sk-123"); // raw error not leaked
    expect(alert).not.toHaveTextContent("/secret/path");
    expect(screen.getByText("delete a file")).toBeInTheDocument(); // still open
    expect(useActivityStore.getState().approvalQueue).toHaveLength(1); // not dequeued
  });

  it("guards against a double-click double-invoke (resolves once)", async () => {
    render(<ApprovalModal />);
    act(() => useActivityStore.getState().enqueueApproval(approval({ approvalId: "a1" })));
    await act(async () => {
      const btn = screen.getByRole("button", { name: /許可/ });
      fireEvent.click(btn);
      fireEvent.click(btn);
    });
    expect(resolveToolApproval).toHaveBeenCalledTimes(1);
  });

  it("renders the CAUTION risk variant", () => {
    render(<ApprovalModal />);
    act(() =>
      useActivityStore
        .getState()
        .enqueueApproval(approval({ approvalId: "a1", risk: "CAUTION", tool: "open_url" })),
    );
    expect(screen.getByText("CAUTION")).toBeInTheDocument();
    expect(document.querySelector(".koe-risk-caution")).not.toBeNull();
  });

  it("auto-dismisses the head request when its deadline passes (no resolve call)", () => {
    vi.useFakeTimers();
    vi.setSystemTime(0);
    render(<ApprovalModal />);
    act(() =>
      useActivityStore.getState().enqueueApproval(approval({ approvalId: "a1", deadlineAt: 1000 })),
    );
    act(() => {
      vi.setSystemTime(1500);
      vi.advanceTimersByTime(1500);
    });
    // Backend already declined on timeout; the UI just clears the modal.
    expect(useActivityStore.getState().approvalQueue).toHaveLength(0);
    expect(resolveToolApproval).not.toHaveBeenCalled();
  });

  // --- a11y / keyboard operability (koe-471) ------------------------------

  it("moves initial focus to the deny (safe) button, not approve (fail-closed)", () => {
    render(<ApprovalModal />);
    act(() => useActivityStore.getState().enqueueApproval(approval({ approvalId: "a1" })));
    expect(screen.getByRole("button", { name: /拒否/ })).toHaveFocus();
    expect(screen.getByRole("button", { name: /許可/ })).not.toHaveFocus();
  });

  it("re-focuses deny when the next queued approval becomes the head", async () => {
    render(<ApprovalModal />);
    act(() => {
      useActivityStore.getState().enqueueApproval(approval({ approvalId: "a1" }));
      useActivityStore.getState().enqueueApproval(approval({ approvalId: "a2" }));
    });
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: /拒否/ })); // resolve a1
    });
    // a2 is now the head — its deny button should hold focus, not approve.
    expect(resolveToolApproval).toHaveBeenLastCalledWith("a1", "deny");
    expect(screen.getByRole("button", { name: /拒否/ })).toHaveFocus();
  });

  it("traps forward Tab at the last control (wraps back to the first)", () => {
    render(<ApprovalModal />);
    act(() => useActivityStore.getState().enqueueApproval(approval({ approvalId: "a1" })));
    const deny = screen.getByRole("button", { name: /拒否/ });
    const approve = screen.getByRole("button", { name: /許可/ });
    act(() => approve.focus());
    fireEvent.keyDown(approve, { key: "Tab" });
    expect(deny).toHaveFocus(); // did not escape the dialog
  });

  it("traps Shift+Tab at the first control (wraps to the last)", () => {
    render(<ApprovalModal />);
    act(() => useActivityStore.getState().enqueueApproval(approval({ approvalId: "a1" })));
    const deny = screen.getByRole("button", { name: /拒否/ });
    const approve = screen.getByRole("button", { name: /許可/ });
    expect(deny).toHaveFocus(); // first control, focused on open
    fireEvent.keyDown(deny, { key: "Tab", shiftKey: true });
    expect(approve).toHaveFocus(); // did not escape the dialog
  });

  it("Escape denies (fail-closed) and dequeues — never approves", async () => {
    render(<ApprovalModal />);
    act(() => useActivityStore.getState().enqueueApproval(approval({ approvalId: "a1" })));
    await act(async () => {
      fireEvent.keyDown(screen.getByRole("button", { name: /拒否/ }), { key: "Escape" });
    });
    expect(resolveToolApproval).toHaveBeenCalledWith("a1", "deny");
    expect(resolveToolApproval).not.toHaveBeenCalledWith("a1", "approve");
    expect(useActivityStore.getState().approvalQueue).toHaveLength(0);
  });

  it("restores focus to the opener when the modal closes", async () => {
    render(
      <>
        <button type="button">opener</button>
        <ApprovalModal />
      </>,
    );
    const opener = screen.getByRole("button", { name: "opener" });
    opener.focus();
    expect(opener).toHaveFocus();
    act(() => useActivityStore.getState().enqueueApproval(approval({ approvalId: "a1" })));
    expect(screen.getByRole("button", { name: /拒否/ })).toHaveFocus(); // focus entered the modal
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: /拒否/ }));
    });
    expect(opener).toHaveFocus(); // focus returned to the caller
  });

  it("announces the remaining time to screen readers via a polite live region", () => {
    render(<ApprovalModal />);
    act(() => useActivityStore.getState().enqueueApproval(approval({ approvalId: "a1" })));
    const live = screen.getByText(/^残り約?\d+秒$/); // SR-only region (not the visible "残り Ns")
    expect(live).toHaveAttribute("aria-live", "polite");
  });
});
