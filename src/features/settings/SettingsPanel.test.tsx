// TDD tests for SettingsPanel component (rhanis-31u: voice provider + 手足 tools).
import { act, fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

const getAppSettings = vi.fn();
const saveBudgetConfig = vi.fn();
const setProviderApiKey = vi.fn();
const hasProviderApiKey = vi.fn();
const deleteProviderApiKey = vi.fn();
const setVoiceProvider = vi.fn();
const setToolProviderEnabled = vi.fn();
const deleteToolProviderKey = vi.fn();

vi.mock("../../lib/tauri/ipc", () => ({
  getAppSettings: (...args: unknown[]) => getAppSettings(...args),
  saveBudgetConfig: (...args: unknown[]) => saveBudgetConfig(...args),
  setProviderApiKey: (...args: unknown[]) => setProviderApiKey(...args),
  hasProviderApiKey: (...args: unknown[]) => hasProviderApiKey(...args),
  deleteProviderApiKey: (...args: unknown[]) => deleteProviderApiKey(...args),
  setVoiceProvider: (...args: unknown[]) => setVoiceProvider(...args),
  setToolProviderEnabled: (...args: unknown[]) => setToolProviderEnabled(...args),
  deleteToolProviderKey: (...args: unknown[]) => deleteToolProviderKey(...args),
  completeOnboarding: vi.fn(),
  setRecorderAdapter: vi.fn(),
}));

import { useSettingsStore } from "./settingsStore";
import { SettingsPanel } from "./SettingsPanel";

const completedSettings = {
  onboarding_completed: true,
  budget: { enabled: true, monthly_limit_nanodollars: 10_000_000_000 },
  recorder_adapter: "sqlite",
  voice_provider_model: "openai/gpt-realtime-2",
  tool_providers: { xai: false, x: false, search: false },
};

beforeEach(() => {
  getAppSettings.mockReset();
  saveBudgetConfig.mockReset();
  setProviderApiKey.mockReset();
  hasProviderApiKey.mockReset();
  deleteProviderApiKey.mockReset();
  setVoiceProvider.mockReset();
  setToolProviderEnabled.mockReset();
  deleteToolProviderKey.mockReset();
  deleteToolProviderKey.mockResolvedValue(undefined);
  getAppSettings.mockResolvedValue(completedSettings);
  saveBudgetConfig.mockResolvedValue(undefined);
  setProviderApiKey.mockResolvedValue(undefined);
  hasProviderApiKey.mockResolvedValue(false);
  deleteProviderApiKey.mockResolvedValue(undefined);
  setVoiceProvider.mockResolvedValue(undefined);
  setToolProviderEnabled.mockResolvedValue(undefined);
  useSettingsStore.setState({
    settings: completedSettings,
    loaded: true,
    loadError: null,
  });
});

async function renderPanel(props = {}) {
  await act(async () => {
    render(<SettingsPanel {...props} />);
  });
}

describe("SettingsPanel", () => {
  it("renders without crashing when settings are loaded", async () => {
    await renderPanel();
    expect(screen.getByRole("region", { name: "設定" })).toBeInTheDocument();
  });

  it("shows the voice provider selector with the persisted value", async () => {
    await renderPanel();
    const combo = screen.getByRole("combobox") as HTMLSelectElement;
    expect(combo.value).toBe("openai/gpt-realtime-2");
  });

  it("renders one key input per 手足 tool plus the OpenAI key (password inputs)", async () => {
    await renderPanel();
    const passwordInputs = document.querySelectorAll('input[type="password"]');
    // OpenAI voice key + xai + x + search = 4
    expect(passwordInputs.length).toBe(4);
  });

  it("shows the 'saved but not yet active' hint for the 手足 tools", async () => {
    await renderPanel();
    expect(screen.getByText(/rhanis-eal/)).toBeInTheDocument();
  });

  it("persists a voice provider change via the store → ipc", async () => {
    await renderPanel();
    const combo = screen.getByRole("combobox");
    await act(async () => {
      fireEvent.change(combo, { target: { value: "openai/gpt-realtime-2" } });
    });
    expect(setVoiceProvider).toHaveBeenCalledWith("openai/gpt-realtime-2");
  });

  it("toggles the first 手足 tool provider via its checkbox (key already stored)", async () => {
    // A stored key is required to enable a tool, so make all keys present.
    hasProviderApiKey.mockResolvedValue(true);
    await renderPanel();
    // With keys stored, each tool row's checkbox label reads exactly "有効にする"
    // (the budget checkbox reads "上限を有効にする", so it is not matched).
    const enableLabels = screen.getAllByText("有効にする");
    expect(enableLabels.length).toBe(3);
    const firstCheckbox = enableLabels[0].closest("label")!.querySelector("input")!;
    expect((firstCheckbox as HTMLInputElement).disabled).toBe(false);
    await act(async () => {
      fireEvent.click(firstCheckbox);
    });
    expect(setToolProviderEnabled).toHaveBeenCalledWith("xai", true);
  });

  it("disables the enable checkbox until a tool key is stored", async () => {
    // beforeEach default: hasProviderApiKey resolves false → no key stored.
    await renderPanel();
    // The gated label is shown and every tool checkbox carries the disabled
    // attribute — the native guard that blocks a user from enabling a key-less
    // tool. (fireEvent bypasses `disabled`, so we assert the attribute itself,
    // which is what actually protects the real browser interaction.)
    const gatedLabels = screen.getAllByText("有効にする（先にキーを保存）");
    expect(gatedLabels.length).toBe(3);
    for (const lbl of gatedLabels) {
      const checkbox = lbl.closest("label")!.querySelector("input")! as HTMLInputElement;
      expect(checkbox.disabled).toBe(true);
    }
  });

  it("clears the enable flag when a tool key is deleted", async () => {
    // xai key is stored AND the tool is enabled.
    hasProviderApiKey.mockResolvedValue(true);
    useSettingsStore.setState({
      settings: { ...completedSettings, tool_providers: { xai: true, x: false, search: false } },
      loaded: true,
      loadError: null,
    });
    await renderPanel();
    // Delete the XAI key → ApiKeyInput fires onKeyStatusChange(false), which must
    // also clear the now-orphaned enable flag (no "enabled but key-less" state).
    const xaiRow = screen.getByText("XAI (Grok) APIキー").closest(".rhanis-tool-key-row")!;
    const deleteBtn = xaiRow.querySelector('button[aria-label="削除"]')! as HTMLButtonElement;
    await act(async () => {
      fireEvent.click(deleteBtn);
    });
    // Tool deletes route through the atomic backend command (key delete + flag
    // clear in one op), NOT the generic deleteProviderApiKey + a separate toggle.
    expect(deleteToolProviderKey).toHaveBeenCalledWith("xai");
    expect(deleteProviderApiKey).not.toHaveBeenCalled();
  });

  it("allows closing the panel via onClose callback", async () => {
    const onClose = vi.fn();
    await renderPanel({ onClose });
    const closeBtn = screen.getByRole("button", { name: /閉じる|close/i });
    await act(async () => {
      fireEvent.click(closeBtn);
    });
    expect(onClose).toHaveBeenCalled();
  });
});
