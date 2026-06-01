// TDD stub — will be filled after implementation is in place.
import { beforeEach, describe, expect, it, vi } from "vitest";

const getAppSettings = vi.fn();
const completeOnboarding = vi.fn();
const saveBudgetConfig = vi.fn();
const setVoiceProvider = vi.fn();
const setToolProviderEnabled = vi.fn();

vi.mock("../../lib/tauri/ipc", () => ({
  getAppSettings: (...args: unknown[]) => getAppSettings(...args),
  completeOnboarding: (...args: unknown[]) => completeOnboarding(...args),
  saveBudgetConfig: (...args: unknown[]) => saveBudgetConfig(...args),
  setVoiceProvider: (...args: unknown[]) => setVoiceProvider(...args),
  setToolProviderEnabled: (...args: unknown[]) => setToolProviderEnabled(...args),
  hasOpenaiApiKey: vi.fn(),
  setOpenaiApiKey: vi.fn(),
  deleteOpenaiApiKey: vi.fn(),
}));

import { useSettingsStore } from "./settingsStore";
import type { AppSettings } from "./types";

const defaultSettings: AppSettings = {
  onboarding_completed: false,
  budget: { enabled: false, monthly_limit_nanodollars: 0 },
  recorder_adapter: "sqlite",
  voice_provider_model: "openai/gpt-realtime-2",
  tool_providers: { xai: false, x: false, search: false },
};

const completedSettings: AppSettings = {
  onboarding_completed: true,
  budget: { enabled: true, monthly_limit_nanodollars: 10_000_000_000 },
  recorder_adapter: "sqlite",
  voice_provider_model: "openai/gpt-realtime-2",
  tool_providers: { xai: false, x: false, search: false },
};

beforeEach(() => {
  getAppSettings.mockReset();
  completeOnboarding.mockReset();
  saveBudgetConfig.mockReset();
  setVoiceProvider.mockReset();
  setToolProviderEnabled.mockReset();
  // Reset zustand store
  useSettingsStore.setState({
    settings: null,
    loaded: false,
    loadError: null,
  });
});

describe("settingsStore.load", () => {
  it("populates settings on success", async () => {
    getAppSettings.mockResolvedValue(defaultSettings);
    await useSettingsStore.getState().load();
    expect(useSettingsStore.getState().settings).toEqual(defaultSettings);
    expect(useSettingsStore.getState().loaded).toBe(true);
    expect(useSettingsStore.getState().loadError).toBeNull();
  });

  it("sets loadError and keeps settings null on failure", async () => {
    getAppSettings.mockRejectedValue(new Error("ipc failed"));
    await useSettingsStore.getState().load();
    expect(useSettingsStore.getState().settings).toBeNull();
    expect(useSettingsStore.getState().loaded).toBe(true);
    expect(useSettingsStore.getState().loadError).toBeTruthy();
  });

  it("does not fabricate a default on failure (fail-closed)", async () => {
    getAppSettings.mockRejectedValue(new Error("ipc failed"));
    await useSettingsStore.getState().load();
    // settings must be null, not a fabricated default
    expect(useSettingsStore.getState().settings).toBeNull();
  });

  it("loadError is the fixed JP string and does not leak the raw error", async () => {
    getAppSettings.mockRejectedValue(new Error("secret/path/leaked details"));
    await useSettingsStore.getState().load();

    const { loadError, settings } = useSettingsStore.getState();
    // Gate must stay closed
    expect(settings).toBeNull();
    // Fixed JP message (not the raw error)
    expect(loadError).toBe("設定の読み込みに失敗しました。");
    expect(loadError).not.toContain("secret");
    expect(loadError).not.toContain("path");
  });
});

describe("settingsStore.completeOnboarding", () => {
  it("calls completeOnboarding IPC with correct args and reloads", async () => {
    completeOnboarding.mockResolvedValue(undefined);
    getAppSettings.mockResolvedValue(completedSettings);

    await useSettingsStore.getState().completeOnboarding(true, 10.0, "sqlite");

    expect(completeOnboarding).toHaveBeenCalledWith(true, 10.0, "sqlite");
    expect(useSettingsStore.getState().settings).toEqual(completedSettings);
  });

  it("propagates IPC errors (does not silently swallow)", async () => {
    completeOnboarding.mockRejectedValue(new Error("invalid budget amount"));
    await expect(
      useSettingsStore.getState().completeOnboarding(true, -1, "sqlite"),
    ).rejects.toThrow();
  });
});

describe("settingsStore.saveBudget", () => {
  it("calls saveBudgetConfig IPC and reloads", async () => {
    saveBudgetConfig.mockResolvedValue(undefined);
    const updated: AppSettings = {
      ...completedSettings,
      budget: { enabled: false, monthly_limit_nanodollars: 0 },
    };
    getAppSettings.mockResolvedValue(updated);

    await useSettingsStore.getState().saveBudget(false, null);

    expect(saveBudgetConfig).toHaveBeenCalledWith(false, null);
    expect(useSettingsStore.getState().settings?.budget.enabled).toBe(false);
  });
});

describe("settingsStore.saveVoiceProvider", () => {
  it("calls setVoiceProvider IPC then re-fetches the authoritative settings", async () => {
    setVoiceProvider.mockResolvedValue(undefined);
    const updated: AppSettings = {
      ...completedSettings,
      voice_provider_model: "google/gemini-2.5-flash-live",
    };
    getAppSettings.mockResolvedValue(updated);

    await useSettingsStore.getState().saveVoiceProvider("google/gemini-2.5-flash-live");

    expect(setVoiceProvider).toHaveBeenCalledWith("google/gemini-2.5-flash-live");
    // Reflects the re-fetched value, not a local optimistic copy (stale guard).
    expect(getAppSettings).toHaveBeenCalled();
    expect(useSettingsStore.getState().settings?.voice_provider_model).toBe(
      "google/gemini-2.5-flash-live",
    );
  });

  it("propagates IPC errors (does not silently swallow)", async () => {
    setVoiceProvider.mockRejectedValue(new Error("unsupported voice provider"));
    await expect(
      useSettingsStore.getState().saveVoiceProvider("evil/model"),
    ).rejects.toThrow();
  });
});

describe("settingsStore.setToolProviderEnabled", () => {
  it("calls setToolProviderEnabled IPC then re-fetches the authoritative settings", async () => {
    setToolProviderEnabled.mockResolvedValue(undefined);
    const updated: AppSettings = {
      ...completedSettings,
      tool_providers: { xai: true, x: false, search: false },
    };
    getAppSettings.mockResolvedValue(updated);

    await useSettingsStore.getState().setToolProviderEnabled("xai", true);

    expect(setToolProviderEnabled).toHaveBeenCalledWith("xai", true);
    expect(getAppSettings).toHaveBeenCalled();
    expect(useSettingsStore.getState().settings?.tool_providers.xai).toBe(true);
  });

  it("propagates IPC errors (does not silently swallow)", async () => {
    setToolProviderEnabled.mockRejectedValue(new Error("unsupported tool provider"));
    await expect(
      useSettingsStore.getState().setToolProviderEnabled("bad", true),
    ).rejects.toThrow();
  });
});
