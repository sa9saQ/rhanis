// Mandatory first-run budget choice. Collects the user's decision and passes
// it upstream via `onBudgetChosen` — NO IPC happens here. The IPC call
// (`complete_onboarding`) fires only after the API-key step completes, in
// OnboardingGate's finalise handler. This prevents `onboarding_completed`
// from flipping to true before the API-key step renders.
//
// Authoritative validation lives in Rust (`usd_to_nanodollars`); the client
// guard here is UX-only (prevents obviously bad values from making the IPC).

import { useState } from "react";

type Choice = "limited" | "unlimited" | null;

interface BudgetOnboardingProps {
  /** Called synchronously (no async) once the user makes a valid choice and
   *  clicks 次へ. The gate stores the values and advances to the API-key step. */
  onBudgetChosen: (enabled: boolean, monthlyLimitUsd: number | null) => void;
}

export function BudgetOnboarding({ onBudgetChosen }: BudgetOnboardingProps) {
  const [choice, setChoice] = useState<Choice>(null);
  const [amountStr, setAmountStr] = useState("");

  const amount = parseFloat(amountStr);
  // UX guard: positive, finite, and below a practical ceiling so 1e308
  // (which is finite!) doesn't slip through to the Rust side unnecessarily.
  const isAmountValid = isFinite(amount) && amount > 0 && amount <= 1_000_000;
  const canSubmit =
    choice === "unlimited" || (choice === "limited" && isAmountValid);

  function handleSubmit() {
    if (!canSubmit || !choice) return;
    const enabled = choice === "limited";
    onBudgetChosen(enabled, enabled ? amount : null);
  }

  return (
    <div className="rhanis-budget-onboarding">
      <h2 className="rhanis-onboarding-title">月次予算の設定</h2>
      <p className="rhanis-onboarding-desc">
        BYOK（自分のOpenAIキーを使う）方式では、高額課金はご自身の負担になります。
        <br />
        音声APIは高価（1分あたり約¥15〜75相当）なので、上限設定を推奨します。
      </p>

      <fieldset className="rhanis-budget-fieldset">
        <legend className="rhanis-visually-hidden">予算の選択</legend>

        {/* aria-label removed: the wrapping <label> + <span> already provide the name */}
        <label className="rhanis-budget-option">
          <input
            type="radio"
            name="budget-choice"
            value="limited"
            checked={choice === "limited"}
            onChange={() => setChoice("limited")}
          />
          <span>上限を設定する</span>
        </label>

        {choice === "limited" && (
          <div className="rhanis-budget-amount">
            <label htmlFor="rhanis-budget-amount-input">月額上限（USD）</label>
            <input
              id="rhanis-budget-amount-input"
              type="number"
              min="0.01"
              max="1000000"
              step="0.01"
              value={amountStr}
              onChange={(e) => setAmountStr(e.target.value)}
              placeholder="例: 10.00"
              className="rhanis-input"
            />
          </div>
        )}

        {/* aria-label removed: the wrapping <label> + <span> already provide the name */}
        <label className="rhanis-budget-option">
          <input
            type="radio"
            name="budget-choice"
            value="unlimited"
            checked={choice === "unlimited"}
            onChange={() => setChoice("unlimited")}
          />
          <span>明示的に無制限（上限なし）</span>
        </label>
      </fieldset>

      <button
        type="button"
        onClick={handleSubmit}
        disabled={!canSubmit}
        className="rhanis-btn rhanis-btn-primary"
        aria-label="次へ"
      >
        次へ
      </button>
    </div>
  );
}
