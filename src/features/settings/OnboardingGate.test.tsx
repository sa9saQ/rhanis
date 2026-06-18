// Tests for OnboardingGate — including the two-step onboarding integration test
// that locks in the fix for the "API-key step unreachable" regression.
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

const getAppSettings = vi.fn();
const completeOnboarding = vi.fn();
const hasOpenaiApiKey = vi.fn();
const setOpenaiApiKey = vi.fn();
// ApiKeyInput (rendered by the gate) now routes through the provider-generic
// commands; OnboardingGate's own mount probe still uses hasOpenaiApiKey.
const setProviderApiKey = vi.fn();
const hasProviderApiKey = vi.fn();
const deleteProviderApiKey = vi.fn();

vi.mock("../../lib/tauri/ipc", () => ({
  getAppSettings: (...args: unknown[]) => getAppSettings(...args),
  completeOnboarding: (...args: unknown[]) => completeOnboarding(...args),
  hasOpenaiApiKey: (...args: unknown[]) => hasOpenaiApiKey(...args),
  setOpenaiApiKey: (...args: unknown[]) => setOpenaiApiKey(...args),
  setProviderApiKey: (...args: unknown[]) => setProviderApiKey(...args),
  hasProviderApiKey: (...args: unknown[]) => hasProviderApiKey(...args),
  deleteProviderApiKey: (...args: unknown[]) => deleteProviderApiKey(...args),
  saveBudgetConfig: vi.fn(),
  deleteOpenaiApiKey: vi.fn(),
  setRecorderAdapter: vi.fn(),
}));

import { useSettingsStore } from "./settingsStore";
import { OnboardingGate } from "./OnboardingGate";

beforeEach(() => {
  getAppSettings.mockReset();
  completeOnboarding.mockReset();
  hasOpenaiApiKey.mockReset();
  setOpenaiApiKey.mockReset();
  setProviderApiKey.mockReset();
  hasProviderApiKey.mockReset();
  deleteProviderApiKey.mockReset();
  // Default: key not stored, finalize resolves
  hasOpenaiApiKey.mockResolvedValue(false);
  setOpenaiApiKey.mockResolvedValue(undefined);
  setProviderApiKey.mockResolvedValue(undefined);
  hasProviderApiKey.mockResolvedValue(false);
  deleteProviderApiKey.mockResolvedValue(undefined);
  completeOnboarding.mockResolvedValue(undefined);
  useSettingsStore.setState({ settings: null, loaded: false, loadError: null });
});

describe("OnboardingGate", () => {
  it("shows a loading indicator while settings are being loaded", async () => {
    getAppSettings.mockReturnValue(new Promise(() => {}));
    render(
      <OnboardingGate>
        <div>app content</div>
      </OnboardingGate>,
    );

    expect(screen.queryByText("app content")).toBeNull();
    const loading =
      document.querySelector('[role="status"]') ?? screen.queryByText(/読み込/i);
    expect(loading).not.toBeNull();
  });

  it("renders the onboarding wizard when onboarding is not completed", async () => {
    getAppSettings.mockResolvedValue({
      onboarding_completed: false,
      budget: { enabled: false, monthly_limit_nanodollars: 0 },
      recorder_adapter: "sqlite",
    });

    await act(async () => {
      render(
        <OnboardingGate>
          <div>app content</div>
        </OnboardingGate>,
      );
    });

    expect(screen.queryByText("app content")).toBeNull();
    expect(screen.getByText(/月次予算/i)).toBeInTheDocument();
  });

  it("renders children when onboarding is completed", async () => {
    getAppSettings.mockResolvedValue({
      onboarding_completed: true,
      budget: { enabled: true, monthly_limit_nanodollars: 10_000_000_000 },
      recorder_adapter: "sqlite",
    });

    await act(async () => {
      render(
        <OnboardingGate>
          <div>app content</div>
        </OnboardingGate>,
      );
    });

    expect(screen.getByText("app content")).toBeInTheDocument();
  });

  it("shows an error state with retry when load fails (does not render the app)", async () => {
    getAppSettings.mockRejectedValue(new Error("ipc failed"));

    await act(async () => {
      render(
        <OnboardingGate>
          <div>app content</div>
        </OnboardingGate>,
      );
    });

    expect(screen.queryByText("app content")).toBeNull();
    expect(screen.getByRole("button", { name: /再試行|retry/i })).toBeInTheDocument();
  });

  // ---- Two-step integration test (regression lock for "API-key step unreachable") ----
  //
  // This test verifies:
  //  1. completing the budget step does NOT flip onboarding_completed (no IPC yet)
  //  2. the API-key step renders after budget is chosen
  //  3. completeOnboarding IPC fires only after the API-key is saved AND 完了 is clicked
  //  4. children render after the reload
  it("two-step flow: budget → api-key → finalize renders children", async () => {
    // Initial state: not onboarded, no key stored.
    getAppSettings
      .mockResolvedValueOnce({
        onboarding_completed: false,
        budget: { enabled: false, monthly_limit_nanodollars: 0 },
        recorder_adapter: "sqlite",
      })
      // After completeOnboarding calls load() internally (via settingsStore.completeOnboarding)
      .mockResolvedValue({
        onboarding_completed: true,
        budget: { enabled: false, monthly_limit_nanodollars: 0 },
        recorder_adapter: "sqlite",
      });

    // The gate's mount probe uses hasOpenaiApiKey (no key yet → false).
    // rhanis-nt2: ApiKeyInput's save now reports presence optimistically via
    // onKeyStatusChange(true) (no has() round-trip), which flips the gate's
    // hasKey and reveals the 完了 button.
    hasOpenaiApiKey.mockResolvedValue(false);

    await act(async () => {
      render(
        <OnboardingGate>
          <div>app content</div>
        </OnboardingGate>,
      );
    });

    // Step 1: Budget step should be visible; API-key step should NOT be yet.
    expect(screen.getByText(/月次予算の設定/i)).toBeInTheDocument();
    expect(screen.queryByText(/OpenAI APIキーを設定/i)).toBeNull();
    // No completeOnboarding call at this point.
    expect(completeOnboarding).not.toHaveBeenCalled();

    // Click 明示的に無制限 then 次へ — advances to step 2 synchronously.
    fireEvent.click(screen.getByText("明示的に無制限（上限なし）").closest("label")!);
    fireEvent.click(screen.getByRole("button", { name: /次へ/i }));

    // Step 2: API-key heading visible; budget step gone.
    expect(screen.getByText(/OpenAI APIキーを設定/i)).toBeInTheDocument();
    expect(screen.queryByText(/月次予算の設定/i)).toBeNull();
    // Still no IPC call.
    expect(completeOnboarding).not.toHaveBeenCalled();

    // Simulate saving the API key: fill input + click 保存.
    const keyInput = document.querySelector("input") as HTMLInputElement;
    fireEvent.change(keyInput, { target: { value: "sk-test-key" } });
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: /保存/i }));
    });

    // After save, ApiKeyInput reports the key present (onKeyStatusChange(true))
    // → 完了 button should appear.
    await waitFor(() => {
      expect(screen.getByRole("button", { name: /完了/i })).toBeInTheDocument();
    });

    // Click 完了 — triggers finalize (completeOnboarding IPC).
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: /完了/i }));
    });

    // completeOnboarding called with the budget choice (unlimited → enabled=false, null)
    expect(completeOnboarding).toHaveBeenCalledWith(false, null, "sqlite");

    // After reload, onboarding_completed=true → children render.
    await waitFor(() => {
      expect(screen.getByText("app content")).toBeInTheDocument();
    });
  });
});
