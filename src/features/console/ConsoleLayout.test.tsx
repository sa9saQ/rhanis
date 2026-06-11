// TDD tests for ConsoleLayout (koe-ios.1) — the glass-box console skeleton:
// collapsible left sidebar (brand / planned destinations / cost + settings at
// the bottom) and the right main column (status-aware greeting → live activity
// panel → voice orb). Brief: docs/design/2026-06-10-glassbox-console-design-brief.md
import { act, fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

const getAppSettings = vi.fn();
const startSessionIpc = vi.fn();

// ConsoleLayout mounts useActivityEvents + useSessionEvents + useCostEvents and
// can open SettingsPanel from the sidebar, so the mock covers the full ipc
// surface those components touch. No Tauri runtime in jsdom.
vi.mock("../../lib/tauri/ipc", () => ({
  onToolEvent: vi.fn().mockResolvedValue(() => {}),
  onThinkingEvent: vi.fn().mockResolvedValue(() => {}),
  onApprovalRequired: vi.fn().mockResolvedValue(() => {}),
  onSessionStatus: vi.fn().mockResolvedValue(() => {}),
  onCostUpdate: vi.fn().mockResolvedValue(() => {}),
  getCostSnapshot: vi.fn().mockResolvedValue({
    month: 202606,
    used_nanodollars: 0,
    limit_nanodollars: null,
    enabled: false,
    over_budget: false,
    sequence: 0,
    used_usd: 0,
    remaining_usd: null,
  }),
  resolveToolApproval: vi.fn().mockResolvedValue(undefined),
  getAppSettings: (...args: unknown[]) => getAppSettings(...args),
  completeOnboarding: vi.fn().mockResolvedValue(undefined),
  saveBudgetConfig: vi.fn().mockResolvedValue(undefined),
  setRecorderAdapter: vi.fn().mockResolvedValue(undefined),
  setOpenaiApiKey: vi.fn().mockResolvedValue(undefined),
  hasOpenaiApiKey: vi.fn().mockResolvedValue(false),
  deleteOpenaiApiKey: vi.fn().mockResolvedValue(undefined),
  setProviderApiKey: vi.fn().mockResolvedValue(undefined),
  hasProviderApiKey: vi.fn().mockResolvedValue(false),
  deleteProviderApiKey: vi.fn().mockResolvedValue(undefined),
  setVoiceProvider: vi.fn().mockResolvedValue(undefined),
  setToolProviderEnabled: vi.fn().mockResolvedValue(undefined),
  deleteToolProviderKey: vi.fn().mockResolvedValue(undefined),
  setPermissionPolicy: vi.fn().mockResolvedValue(undefined),
  pickFolder: vi.fn().mockResolvedValue(null),
  startSession: (...args: unknown[]) => startSessionIpc(...args),
  stopSession: vi.fn().mockResolvedValue(undefined),
}));

import { useActivityStore } from "../activity/activityStore";
import { useCostStore } from "../activity/costStore";
import { useSessionStore } from "../session/sessionStore";
import { useSettingsStore } from "../settings/settingsStore";
import { ConsoleLayout } from "./ConsoleLayout";

const completedSettings = {
  onboarding_completed: true,
  budget: { enabled: false, monthly_limit_nanodollars: 0 },
  recorder_adapter: "sqlite",
  voice_provider_model: "openai/gpt-realtime-2",
  tool_providers: { xai: false, x: false, search: false },
};

/** An over-budget cost snapshot used by the auto-reopen test. */
const overBudgetSnapshot = {
  month: 202606,
  used_nanodollars: 12_000_000_000,
  limit_nanodollars: 10_000_000_000,
  enabled: true,
  over_budget: true,
  sequence: 7,
  used_usd: 12,
  remaining_usd: 0,
};

beforeEach(() => {
  getAppSettings.mockReset();
  getAppSettings.mockResolvedValue(completedSettings);
  startSessionIpc.mockReset();
  startSessionIpc.mockResolvedValue(undefined);
  useSettingsStore.setState({
    settings: completedSettings,
    loaded: true,
    loadError: null,
  });
  useSessionStore.getState().reset();
  useActivityStore.getState().reset();
  useCostStore.getState().reset();
});

async function renderConsole() {
  await act(async () => {
    render(<ConsoleLayout />);
  });
}

describe("ConsoleLayout — shell", () => {
  it("renders the sidebar with brand, planned destinations and the cost line", async () => {
    await renderConsole();

    const sidebar = screen.getByRole("complementary", { name: "サイドバー" });
    expect(sidebar).toBeInTheDocument();
    expect(screen.getByText("koe")).toBeInTheDocument();

    // 新しい会話 works today: a real button, enabled while idle.
    expect(
      screen.getByRole("button", { name: "新しい会話" }),
    ).not.toBeDisabled();

    // Planned destinations from the approved brief layout — visible but
    // honestly marked as upcoming (their features ship in separate issues).
    expect(screen.getByText("近日追加")).toBeInTheDocument();
    for (const label of [
      "検索",
      "プロジェクト",
      "オートメーション",
      "手足ツール",
      "タスクボード",
    ]) {
      expect(screen.getByText(label)).toBeInTheDocument();
    }

    // CostHeader sits at the sidebar foot ("今月: …" while the first pull is
    // in flight — fail-closed display comes from the component itself).
    expect(screen.getByText(/今月:/)).toBeInTheDocument();
  });

  it("renders the main column: greeting, activity panel and voice button", async () => {
    await renderConsole();

    expect(
      screen.getByRole("heading", { level: 1, name: "今日は何をしましょう？" }),
    ).toBeInTheDocument();
    // ActivityLog (the hero panel) renders its idle status.
    expect(screen.getByText("待機")).toBeInTheDocument();
    // VoiceButton (the shrunken orb) is the idle start control.
    expect(
      screen.getByRole("button", { name: /セッションを開始/ }),
    ).toBeInTheDocument();
  });
});

describe("ConsoleLayout — greeting follows the session status", () => {
  it("invites speech while connected", async () => {
    await renderConsole();
    act(() => {
      useSessionStore.setState({ status: "connected" });
    });
    expect(
      screen.getByRole("heading", { level: 1, name: "どうぞ、話しかけてください" }),
    ).toBeInTheDocument();
  });

  it("announces reconnection while reconnecting", async () => {
    await renderConsole();
    act(() => {
      useSessionStore.setState({ status: "reconnecting" });
    });
    expect(
      screen.getByRole("heading", { level: 1, name: "再接続しています…" }),
    ).toBeInTheDocument();
  });

  it("does not show a false-normal greeting on error", async () => {
    await renderConsole();
    act(() => {
      useSessionStore.setState({ status: "error", error: "boom" });
    });
    expect(
      screen.getByRole("heading", { level: 1, name: "接続に問題があります" }),
    ).toBeInTheDocument();
  });
});

describe("ConsoleLayout — 新しい会話", () => {
  it("starts a session from the sidebar while idle", async () => {
    await renderConsole();
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "新しい会話" }));
    });
    expect(startSessionIpc).toHaveBeenCalledTimes(1);
  });

  it("is disabled while a session is live", async () => {
    await renderConsole();
    act(() => {
      useSessionStore.setState({ status: "connected" });
    });
    expect(screen.getByRole("button", { name: "新しい会話" })).toBeDisabled();
  });
});

describe("ConsoleLayout — settings", () => {
  it("opens the settings panel from the sidebar and closes it again", async () => {
    await renderConsole();

    expect(screen.queryByRole("region", { name: "設定" })).not.toBeInTheDocument();

    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "設定" }));
    });
    expect(screen.getByRole("region", { name: "設定" })).toBeInTheDocument();

    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "閉じる" }));
    });
    expect(screen.queryByRole("region", { name: "設定" })).not.toBeInTheDocument();
  });
});

describe("ConsoleLayout — sidebar collapse", () => {
  it("collapses and reopens the sidebar from the toggle", async () => {
    await renderConsole();

    const toggle = screen.getByRole("button", { name: "サイドバーを開閉" });
    expect(toggle).toHaveAttribute("aria-expanded", "true");

    fireEvent.click(toggle);
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    expect(
      screen.queryByRole("complementary", { name: "サイドバー" }),
    ).not.toBeInTheDocument();

    fireEvent.click(toggle);
    expect(toggle).toHaveAttribute("aria-expanded", "true");
    expect(
      screen.getByRole("complementary", { name: "サイドバー" }),
    ).toBeInTheDocument();
  });

  it("reopens a collapsed sidebar when the budget goes over (stop notice must be visible)", async () => {
    await renderConsole();

    fireEvent.click(screen.getByRole("button", { name: "サイドバーを開閉" }));
    expect(
      screen.queryByRole("complementary", { name: "サイドバー" }),
    ).not.toBeInTheDocument();

    // The over-budget stop + raise control lives in the sidebar's CostHeader;
    // it must never be hidden behind a collapsed sidebar (fail-closed UX).
    act(() => {
      useCostStore.getState().applySnapshot(overBudgetSnapshot);
    });
    expect(
      screen.getByRole("complementary", { name: "サイドバー" }),
    ).toBeInTheDocument();
    expect(
      screen.getByText(/予算上限に達したため会話を停止しました/),
    ).toBeInTheDocument();
  });

  it("cannot hide the stop notice by collapsing while over budget", async () => {
    await renderConsole();

    act(() => {
      useCostStore.getState().applySnapshot(overBudgetSnapshot);
    });

    // The toggle is disabled (an enabled toggle that ignores clicks would just
    // look broken); the sidebar — and the raise control — stay visible for the
    // whole over-budget episode, not only at the false→true transition.
    const toggle = screen.getByRole("button", { name: "サイドバーを開閉" });
    expect(toggle).toBeDisabled();
    fireEvent.click(toggle);
    expect(
      screen.getByRole("complementary", { name: "サイドバー" }),
    ).toBeInTheDocument();
    expect(
      screen.getByText(/予算上限に達したため会話を停止しました/),
    ).toBeInTheDocument();

    // And starting a conversation is disabled — the backend would reject it,
    // so the UI must not offer 開始 next to the stop notice (R-C finding).
    expect(screen.getByRole("button", { name: "新しい会話" })).toBeDisabled();
  });
});
