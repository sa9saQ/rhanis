import { act, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";

import { ActivityLog } from "./ActivityLog";
import { useActivityStore } from "./activityStore";
import type { ToolEvent } from "./types";

function ev(partial: Pick<ToolEvent, "eventId" | "actionId" | "sequence" | "phase">): ToolEvent {
  return {
    tool: "web_search",
    timestamp: 1000,
    displaySummary: "searching the web",
    ...partial,
  };
}

beforeEach(() => {
  useActivityStore.getState().reset();
});

describe("ActivityLog", () => {
  it("shows the idle status and empty states by default", () => {
    render(<ActivityLog />);
    expect(screen.getByText("待機")).toBeInTheDocument();
    expect(screen.getByText("いまは静かです")).toBeInTheDocument();
    expect(screen.getByText("まだ記録はありません")).toBeInTheDocument();
  });

  it("renders a live action and the working status while a tool runs", () => {
    render(<ActivityLog />);
    act(() => {
      useActivityStore.getState().setSessionStatus({ state: "connected", sequence: 1 });
      useActivityStore
        .getState()
        .ingestToolEvent(ev({ eventId: "e1", actionId: "a1", sequence: 1, phase: "start" }));
    });
    expect(screen.getByText("作業")).toBeInTheDocument();
    // tool name appears in both the live row and the log row.
    expect(screen.getAllByText("web_search").length).toBeGreaterThanOrEqual(1);
  });

  it("shows 再接続中 while the session is reconnecting (rhanis-byf)", () => {
    render(<ActivityLog />);
    act(() => {
      useActivityStore.getState().setSessionStatus({ state: "reconnecting", sequence: 1 });
    });
    expect(screen.getByText("再接続中")).toBeInTheDocument();
    // Not a terminal state: no error alert is shown.
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("surfaces a provider/server error without flipping the session status (rhanis-nal)", () => {
    render(<ActivityLog />);
    act(() => {
      useActivityStore.getState().setSessionStatus({ state: "connected", sequence: 1 });
      useActivityStore.getState().ingestProviderError({
        eventId: "p1",
        sequence: 1,
        code: "unknown_parameter",
        message: "Unknown parameter: 'session.bogus'.",
        timestamp: 1000,
      });
    });
    // The error row is visible (code + message rendered as plain text)…
    expect(
      screen.getByText(/サーバーエラー \(unknown_parameter\): Unknown parameter/),
    ).toBeInTheDocument();
    // …but the session is NOT shown as dead: still conversing, no terminal alert.
    expect(screen.getByText("会話")).toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("keeps the provider-error live region mounted (empty) for reliable announcement", () => {
    render(<ActivityLog />);
    expect(screen.getByRole("list", { name: "サーバーエラー" })).toBeInTheDocument();
  });

  it("shows the WHY of an error row via the backend detail (rhanis-r2o)", () => {
    render(<ActivityLog />);
    act(() => {
      useActivityStore.getState().ingestToolEvent({
        ...ev({ eventId: "e1", actionId: "a1", sequence: 1, phase: "error" }),
        detail: "tool not implemented",
      });
    });
    expect(screen.getByText("tool not implemented")).toBeInTheDocument();
  });

  it("shows the pending-approval badge", () => {
    render(<ActivityLog />);
    act(() => {
      useActivityStore.getState().enqueueApproval({
        approvalId: "ap1",
        tool: "run_command",
        risk: "DANGER",
        displaySummary: "danger op",
        deadlineAt: 30_000,
        sequence: 1,
      });
    });
    expect(screen.getByText(/承認待ち 1/)).toBeInTheDocument();
  });

  it("surfaces the error message in error state", () => {
    render(<ActivityLog />);
    act(() => {
      useActivityStore
        .getState()
        .setSessionStatus({ state: "error", error: "接続に失敗しました", sequence: 1 });
    });
    expect(screen.getByText("エラー")).toBeInTheDocument();
    expect(screen.getByRole("alert")).toHaveTextContent("接続に失敗しました");
  });

  it("keeps the thinking live region mounted (empty) so the first disclosure is announced", () => {
    render(<ActivityLog />);
    // The aria-live region must already exist before content arrives (a11y), so it
    // is always mounted — present but carrying no disclosure rows when idle.
    const region = screen.getByLabelText("考えていること");
    expect(region).toBeInTheDocument();
    expect(region.querySelectorAll("li")).toHaveLength(0);
  });

  it("discloses what Rhanis is about to do, with the verifiable tool (glass-box M1)", () => {
    render(<ActivityLog />);
    act(() => {
      useActivityStore.getState().ingestThinkingEvent({
        eventId: "t1",
        actionId: "a1",
        sequence: 1,
        phase: "deciding",
        plan: "ウェブを検索しています",
        tool: "web_search",
        source: "web",
        timestamp: 1000,
      });
    });
    expect(screen.getByLabelText("考えていること")).toBeInTheDocument();
    expect(screen.getByText("ウェブを検索しています")).toBeInTheDocument();
    expect(screen.getByText("web_search")).toBeInTheDocument();
  });
});
