// Onboarding gate — blocks the activity console until onboarding is complete.
//
// Responsibility: UI gate enforcing `onboarding_completed = true`.
// Backend enforcement: session_manager (rhanis-e3m) reads `onboarding_completed`
// and `cost_tracker::can_start_session()` before allowing a session to start.
// This is a deliberate seam — rhanis-200 owns the UI gate, rhanis-e3m owns the
// session-start block.
//
// Two-step flow (CRITICAL — do not collapse into one step):
//   Step 1: BudgetOnboarding — collects choice synchronously, no IPC.
//   Step 2: ApiKeyInput — user stores the API key.
//   Finalise: `complete_onboarding` IPC fires HERE after both steps.
//             Only at this point does `onboarding_completed` flip to true.
// Collapsing steps breaks BYOK: if the flag flipped after step 1, the gate
// would pass through to children before the API key is stored.

import { useEffect, useRef, useState } from "react";

import { hasOpenaiApiKey } from "../../lib/tauri/ipc";
import { useSettingsStore } from "./settingsStore";
import { BudgetOnboarding } from "./BudgetOnboarding";
import { ApiKeyInput } from "./ApiKeyInput";
// Onboarding is the first screen, so it loads its own styles directly: the shared
// form vocabulary (settings.css) + the first-run layout (onboarding.css). It does
// NOT rely on another feature's stylesheet being in the boot bundle (rhanis-iyr).
import "./settings.css";
import "./onboarding.css";

interface OnboardingGateProps {
  children: React.ReactNode;
}

export function OnboardingGate({ children }: OnboardingGateProps) {
  const { settings, loaded, loadError, load, completeOnboarding } = useSettingsStore();

  /** Budget choice collected in step 1. null = step 1 not yet completed. */
  const [budgetChoice, setBudgetChoice] = useState<{
    enabled: boolean;
    monthlyLimitUsd: number | null;
  } | null>(null);

  const [hasKey, setHasKey] = useState(false);
  const [finalizing, setFinalizing] = useState(false);
  const [finalizeError, setFinalizeError] = useState<string | null>(null);
  const inFlightFinalize = useRef(false);

  useEffect(() => {
    void load();
  }, [load]);

  // On mount also check whether a key is already stored (returning user who
  // stored a key in a previous partial session).
  useEffect(() => {
    void hasOpenaiApiKey()
      .then(setHasKey)
      .catch(() => {
        /* best-effort; a failed check just leaves the Complete button hidden */
      });
  }, []);

  async function handleFinalize() {
    if (!budgetChoice || inFlightFinalize.current) return;
    inFlightFinalize.current = true;
    setFinalizing(true);
    setFinalizeError(null);
    try {
      await completeOnboarding(budgetChoice.enabled, budgetChoice.monthlyLimitUsd, "sqlite");
      // The store's completeOnboarding re-fetches settings (getAppSettings) after
      // the IPC, so settings.onboarding_completed flips → true; this component
      // re-renders and passes through to children. (If that re-fetch throws, the
      // catch below shows finalizeError; the Rust write is idempotent, so a
      // retry is safe.)
    } catch {
      // Do NOT surface the raw IPC error — it may carry backend detail / PII.
      setFinalizeError("オンボーディングの完了に失敗しました。もう一度お試しください。");
    } finally {
      inFlightFinalize.current = false;
      setFinalizing(false);
    }
  }

  if (!loaded) {
    return (
      <div
        className="rhanis-onboarding-gate-loading"
        aria-label="読み込み中"
        role="status"
        aria-live="polite"
      >
        <p>読み込み中…</p>
      </div>
    );
  }

  if (loadError) {
    return (
      <div className="rhanis-onboarding-gate-error">
        <p role="alert">{loadError}</p>
        <button
          type="button"
          onClick={() => void load()}
          className="rhanis-btn"
          aria-label="再試行"
        >
          再試行
        </button>
      </div>
    );
  }

  if (settings?.onboarding_completed) {
    return <>{children}</>;
  }

  // Show onboarding wizard — two steps
  return (
    <div className="rhanis-onboarding-wizard">
      <h1 className="rhanis-onboarding-heading">Rhanis へようこそ</h1>

      {budgetChoice === null ? (
        // Step 1: Budget choice (synchronous — no IPC)
        <BudgetOnboarding
          onBudgetChosen={(enabled, monthlyLimitUsd) =>
            setBudgetChoice({ enabled, monthlyLimitUsd })
          }
        />
      ) : (
        // Step 2: API key entry
        <div className="rhanis-onboarding-apikey-step">
          <h2 className="rhanis-onboarding-title">OpenAI APIキーを設定</h2>
          <p className="rhanis-onboarding-desc">
            Rhanis はあなた自身のOpenAI APIキーを使用します（BYOK方式）。
            <br />
            キーは暗号化されたローカルストレージに保存され、外部には送信されません。
          </p>
          <ApiKeyInput
            hasKey={hasKey}
            onKeyStatusChange={(k) => setHasKey(k)}
          />

          {finalizeError && (
            <p role="alert" className="rhanis-onboarding-error">
              {finalizeError}
            </p>
          )}

          {hasKey && (
            <button
              type="button"
              onClick={() => void handleFinalize()}
              disabled={finalizing}
              className="rhanis-btn rhanis-btn-primary"
            >
              {finalizing ? "保存中…" : "完了"}
            </button>
          )}

          <button
            type="button"
            onClick={() => setBudgetChoice(null)}
            disabled={finalizing}
            className="rhanis-btn"
          >
            ← 戻る
          </button>
        </div>
      )}
    </div>
  );
}
