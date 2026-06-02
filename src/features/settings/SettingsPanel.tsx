// Post-onboarding settings panel. Lets the user pick the voice provider, enter
// per-provider API keys (OpenAI voice key + 手足 tool keys), toggle which tools
// are enabled, and re-edit the budget.

import { useEffect, useRef, useState } from "react";

import { hasProviderApiKey, type ToolProvider } from "../../lib/tauri/ipc";
import { useSettingsStore } from "./settingsStore";
import { ApiKeyInput } from "./ApiKeyInput";
import { VoiceProviderSelector } from "./VoiceProviderSelector";
import { nanodollarsToUsdDisplay } from "./utils";
import type { ToolProviderFlags } from "./types";
import "./settings.css";

interface SettingsPanelProps {
  onClose?: () => void;
}

// 手足 (tool) providers shown in the settings list. The provider id matches the
// backend allowlist. Adding a tool = appending here (plus the secret_store
// allowlist + a ToolProviderFlags field). Keys are STORED now but only USED once
// koe-eal wires the tools — made explicit to the user in the section hint below.
const TOOL_KEYS: { provider: keyof ToolProviderFlags; label: string; placeholder?: string }[] = [
  { provider: "xai", label: "XAI (Grok) APIキー", placeholder: "xai-…" },
  // X / search keys have no well-known prefix → a neutral placeholder (not the
  // OpenAI "sk-…" default) avoids a misleading hint.
  { provider: "x", label: "X API キー", placeholder: "API キーを貼り付け" },
  { provider: "search", label: "検索 API キー", placeholder: "API キーを貼り付け" },
];

const TOOL_KEYS_HINT_ID = "koe-tool-keys-hint";

const DEFAULT_VOICE_PROVIDER_MODEL = "openai/gpt-realtime-2";

export function SettingsPanel({ onClose }: SettingsPanelProps) {
  const { settings, saveBudget, saveVoiceProvider, setToolProviderEnabled, deleteToolProviderKey } =
    useSettingsStore();
  const [hasKey, setHasKey] = useState(false);
  const [toolHasKey, setToolHasKey] = useState<Record<string, boolean>>({});
  const [actionError, setActionError] = useState<string | null>(null);
  const [savingBudget, setSavingBudget] = useState(false);
  const [budgetError, setBudgetError] = useState<string | null>(null);
  const [newLimitStr, setNewLimitStr] = useState(
    settings?.budget.enabled
      ? String(nanodollarsToUsdDisplay(settings.budget.monthly_limit_nanodollars))
      : "",
  );
  const [budgetEnabled, setBudgetEnabled] = useState(settings?.budget.enabled ?? false);
  // Re-entrancy guard: prevents double-submit if the button state lags a render.
  const inFlightBudget = useRef(false);

  // Check the real stored-key state on open (OpenAI voice key + each tool key)
  // so the delete button / "✓ saved" indicators reflect the vault.
  useEffect(() => {
    void hasProviderApiKey("openai")
      .then(setHasKey)
      .catch(() => {
        /* best-effort; failure just leaves the indicator absent */
      });
    for (const { provider } of TOOL_KEYS) {
      void hasProviderApiKey(provider)
        .then((has) => setToolHasKey((m) => ({ ...m, [provider]: has })))
        .catch(() => {
          /* best-effort */
        });
    }
  }, []);

  const voiceModel = settings?.voice_provider_model ?? DEFAULT_VOICE_PROVIDER_MODEL;
  const toolFlags = settings?.tool_providers ?? { xai: false, x: false, search: false };

  async function handleVoiceChange(value: string) {
    setActionError(null);
    try {
      await saveVoiceProvider(value);
    } catch {
      setActionError("声のプロバイダの保存に失敗しました。もう一度お試しください。");
    }
  }

  async function handleToolToggle(provider: ToolProvider, enabled: boolean) {
    setActionError(null);
    try {
      await setToolProviderEnabled(provider, enabled);
    } catch {
      setActionError("ツールの設定保存に失敗しました。もう一度お試しください。");
    }
  }

  async function handleSaveBudget() {
    if (inFlightBudget.current || savingBudget) return;
    inFlightBudget.current = true;
    setSavingBudget(true);
    setBudgetError(null);
    try {
      const limit = budgetEnabled ? parseFloat(newLimitStr) : null;
      if (budgetEnabled && (!isFinite(limit!) || limit! <= 0 || limit! > 1_000_000)) {
        setBudgetError("有効な金額を入力してください（0〜1,000,000 USD）。");
        return;
      }
      await saveBudget(budgetEnabled, limit);
    } catch {
      setBudgetError("予算の保存に失敗しました。もう一度お試しください。");
    } finally {
      inFlightBudget.current = false;
      setSavingBudget(false);
    }
  }

  return (
    <div className="koe-settings-panel" role="region" aria-label="設定">
      <div className="koe-settings-header">
        <h2 className="koe-settings-title">設定</h2>
        <button
          type="button"
          onClick={onClose}
          className="koe-btn koe-btn-icon"
          aria-label="閉じる"
        >
          ✕
        </button>
      </div>

      <section className="koe-settings-section">
        <h3>声のプロバイダ</h3>
        <VoiceProviderSelector value={voiceModel} onChange={(v) => void handleVoiceChange(v)} />
        <ApiKeyInput
          provider="openai"
          label="OpenAI APIキー"
          hasKey={hasKey}
          onKeyStatusChange={setHasKey}
        />
      </section>

      <section className="koe-settings-section">
        <h3>手足ツール（API キー）</h3>
        <p id={TOOL_KEYS_HINT_ID} className="koe-settings-hint">
          これらのキーは保存されますが、AI が裏で実際に使う機能は順次追加します（koe-eal）。
        </p>
        {TOOL_KEYS.map(({ provider, label, placeholder }) => {
          const keyStored = toolHasKey[provider] ?? false;
          return (
            <div key={provider} className="koe-tool-key-row">
              <ApiKeyInput
                provider={provider}
                label={label}
                placeholder={placeholder}
                hasKey={keyStored}
                onKeyStatusChange={(has) => setToolHasKey((m) => ({ ...m, [provider]: has }))}
                onDelete={() => deleteToolProviderKey(provider)}
                describedById={TOOL_KEYS_HINT_ID}
              />
              <label className="koe-tool-enable">
                <input
                  type="checkbox"
                  checked={toolFlags[provider] ?? false}
                  // Can't enable a tool with no stored credential — enabling a
                  // key-less provider would leave a flag a future consumer
                  // (koe-eal) trusts with nothing behind it.
                  disabled={!keyStored}
                  onChange={(e) => void handleToolToggle(provider, e.target.checked)}
                  aria-describedby={TOOL_KEYS_HINT_ID}
                />
                <span>{keyStored ? "有効にする" : "有効にする（先にキーを保存）"}</span>
              </label>
            </div>
          );
        })}
      </section>

      {actionError && (
        <p role="alert" className="koe-settings-error">
          {actionError}
        </p>
      )}

      <section className="koe-settings-section">
        <h3>月次予算</h3>

        <label className="koe-budget-option">
          <input
            type="checkbox"
            checked={budgetEnabled}
            onChange={(e) => setBudgetEnabled(e.target.checked)}
            disabled={savingBudget}
          />
          <span>上限を有効にする</span>
        </label>

        {budgetEnabled && (
          <div className="koe-budget-amount">
            <label htmlFor="koe-settings-budget-input">月額上限（USD）</label>
            <input
              id="koe-settings-budget-input"
              type="number"
              min="0.01"
              max="1000000"
              step="0.01"
              value={newLimitStr}
              onChange={(e) => setNewLimitStr(e.target.value)}
              disabled={savingBudget}
              className="koe-input"
            />
          </div>
        )}

        {budgetError && (
          <p role="alert" className="koe-settings-error">
            {budgetError}
          </p>
        )}

        <button
          type="button"
          onClick={() => void handleSaveBudget()}
          disabled={savingBudget}
          className="koe-btn koe-btn-primary"
        >
          {savingBudget ? "保存中…" : "予算を保存"}
        </button>
      </section>
    </div>
  );
}
