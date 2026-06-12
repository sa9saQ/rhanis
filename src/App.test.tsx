import { act, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

const getAppSettings = vi.fn();

// App mounts useActivityEvents + useSessionEvents (subscribe via ipc) and
// OnboardingGate (loads settings). Mock all ipc functions so no Tauri runtime
// is required. VoiceButton also reads sessionStore which imports start/stopSession
// — those must be in the mock to avoid "not a function" errors.
vi.mock("./lib/tauri/ipc", () => ({
  onToolEvent: vi.fn().mockResolvedValue(() => {}),
  // Thinking-event wiring (glass-box M1, koe-sua.1) — useActivityEvents subscribes.
  onThinkingEvent: vi.fn().mockResolvedValue(() => {}),
  onProviderError: vi.fn().mockResolvedValue(() => {}),
  onApprovalRequired: vi.fn().mockResolvedValue(() => {}),
  onSessionStatus: vi.fn().mockResolvedValue(() => {}),
  // Cost snapshot wiring (koe-9xi) — useCostEvents subscribes + pulls on mount.
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
  // Session lifecycle (used by sessionStore, which VoiceButton reads).
  startSession: vi.fn().mockResolvedValue(undefined),
  stopSession: vi.fn().mockResolvedValue(undefined),
}));

import { useSessionStore } from "./features/session/sessionStore";
import { useSettingsStore } from "./features/settings/settingsStore";
import App from "./App";
import { onToolEvent } from "./lib/tauri/ipc";

beforeEach(() => {
  getAppSettings.mockReset();
  // Default: onboarding completed so the app renders the activity console.
  getAppSettings.mockResolvedValue({
    onboarding_completed: true,
    budget: { enabled: false, monthly_limit_nanodollars: 0 },
    recorder_adapter: "sqlite",
  });
  useSettingsStore.setState({ settings: null, loaded: false, loadError: null });
  useSessionStore.getState().reset();
});

describe("App", () => {
  it("renders the activity console and wires up event subscriptions", async () => {
    await act(async () => {
      render(<App />);
    });
    // The console shell (koe-ios.1) renders the idle greeting as its h1 anchor.
    expect(
      screen.getByRole("heading", { level: 1, name: "今日は何をしましょう？" }),
    ).toBeInTheDocument();
    // ActivityLog renders with the default idle status.
    expect(screen.getByText("待機")).toBeInTheDocument();
    await waitFor(() => expect(onToolEvent).toHaveBeenCalled());
  });

  it("renders the VoiceButton in idle state", async () => {
    await act(async () => {
      render(<App />);
    });
    // VoiceButton is in the document: the start button aria-label indicates idle.
    const voiceBtn = screen.getByRole("button", { name: /セッションを開始/i });
    expect(voiceBtn).toBeInTheDocument();
    expect(voiceBtn).not.toBeDisabled();
  });
});
