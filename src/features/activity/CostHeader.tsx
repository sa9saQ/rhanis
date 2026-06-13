// Live monthly-cost header (rhanis-9xi). Shows "今月: $X.XX / 上限 $Y.YY" (or 上限なし
// when the budget is disabled), folding the costStore snapshot that arrives via
// pull (get_cost_snapshot) + push (cost-update). When the backend reports
// over_budget it renders a stop notice + an inline "raise the cap" recovery form.
//
// Invariant (rhanis-9xi): `over_budget` is decided in Rust (u64). This component only
// DISPLAYS the backend's bool + the f64 USD fields — it never recomputes the
// over-budget state. FAIL-CLOSED display: on a load failure with no snapshot it
// shows an explicit "取得できません", never a fabricated $0.

import { useState, type FormEvent } from "react";

import { getCostSnapshot } from "../../lib/tauri/ipc";
import "./CostHeader.css";
import { useSettingsStore } from "../settings/settingsStore";
import { nanodollarsToUsdDisplay } from "../settings/utils";
import { useCostStore } from "./costStore";

/** UI guard mirroring the Rust ceiling (the backend is authoritative). */
const MAX_LIMIT_USD = 1_000_000;

function usd(n: number): string {
  return `$${n.toFixed(2)}`;
}

export function CostHeader() {
  const snapshot = useCostStore((s) => s.snapshot);
  const loadError = useCostStore((s) => s.loadError);

  const [limitStr, setLimitStr] = useState("");
  const [busy, setBusy] = useState(false);
  const [raiseError, setRaiseError] = useState<string | null>(null);

  async function handleRaise(e: FormEvent) {
    e.preventDefault();
    if (busy) return;
    const limit = parseFloat(limitStr);
    // UX guard only — the Rust side validates authoritatively.
    if (!isFinite(limit) || limit <= 0 || limit > MAX_LIMIT_USD) {
      setRaiseError("有効な金額を入力してください（0〜1,000,000 USD）。");
      return;
    }
    setBusy(true);
    setRaiseError(null);
    // `saved` distinguishes the two awaits: once the cap is persisted, a failure of
    // the display re-pull is NOT a raise failure (the cap IS saved). Reporting
    // "raise failed" there would be misleading and invite a confused double-raise —
    // and is fail-closed regardless (the stale over-budget stop UI simply persists
    // until the next live push / pull refreshes it).
    let saved = false;
    try {
      // Persist the new cap (Rust validates authoritatively + reloads settings).
      await useSettingsStore.getState().saveBudget(true, limit);
      saved = true;
      // Refresh the header from the authoritative snapshot (clears the stop UI
      // once the higher cap is reflected). Clear the input only on full success.
      const snap = await getCostSnapshot();
      useCostStore.getState().applySnapshot(snap);
      setLimitStr("");
    } catch {
      setRaiseError(
        saved
          ? "上限を保存しました。表示の更新に失敗しました。再読込してください。"
          : "上限の保存に失敗しました。もう一度お試しください。",
      );
    } finally {
      setBusy(false);
    }
  }

  // No snapshot yet: explicit "unknown" on failure (fail-closed — never $0), or a
  // quiet placeholder while the first pull is in flight.
  if (!snapshot) {
    return (
      <span className="rhanis-cost rhanis-cost-unknown">
        {loadError ? "今月: 取得できません" : "今月: …"}
      </span>
    );
  }

  const capLabel =
    snapshot.enabled && snapshot.limit_nanodollars != null
      ? ` / 上限 ${usd(nanodollarsToUsdDisplay(snapshot.limit_nanodollars))}`
      : " / 上限なし";

  return (
    <div className="rhanis-cost">
      <span className="rhanis-cost-line">{`今月: ${usd(snapshot.used_usd)}${capLabel}`}</span>

      {snapshot.over_budget && (
        <div className="rhanis-cost-overbudget">
          <p className="rhanis-cost-stop" role="alert">
            予算上限に達したため会話を停止しました。続けるには上限を引き上げてください。
          </p>
          <form className="rhanis-cost-raise" onSubmit={(e) => void handleRaise(e)}>
            <label htmlFor="rhanis-raise-limit">新しい月額上限（USD）</label>
            <input
              id="rhanis-raise-limit"
              type="number"
              min="0.01"
              max="1000000"
              step="0.01"
              value={limitStr}
              onChange={(e) => setLimitStr(e.target.value)}
              disabled={busy}
              className="rhanis-cost-input"
            />
            <button type="submit" disabled={busy} className="rhanis-cost-raise-btn">
              {busy ? "保存中…" : "上限を引き上げる"}
            </button>
          </form>
          {raiseError && (
            <p className="rhanis-cost-error" role="alert">
              {raiseError}
            </p>
          )}
        </div>
      )}
    </div>
  );
}
