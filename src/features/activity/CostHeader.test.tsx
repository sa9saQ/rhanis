import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { CostSnapshot } from "./types";

// Mock the ipc surface the raise flow touches: settingsStore.saveBudget calls
// saveBudgetConfig + getAppSettings; CostHeader then re-fetches getCostSnapshot.
const getCostSnapshot = vi.fn();
const saveBudgetConfig = vi.fn();
const getAppSettings = vi.fn();
vi.mock("../../lib/tauri/ipc", () => ({
  getCostSnapshot: () => getCostSnapshot(),
  saveBudgetConfig: (...args: unknown[]) => saveBudgetConfig(...args),
  getAppSettings: () => getAppSettings(),
}));

import { useCostStore } from "./costStore";
import { CostHeader } from "./CostHeader";

function snapshot(over: Partial<CostSnapshot> = {}): CostSnapshot {
  return {
    month: 202606,
    used_nanodollars: 16_000_000_000,
    limit_nanodollars: 32_000_000_000,
    enabled: true,
    over_budget: false,
    sequence: 1,
    used_usd: 16,
    remaining_usd: 16,
    ...over,
  };
}

beforeEach(() => {
  getCostSnapshot.mockReset();
  saveBudgetConfig.mockReset().mockResolvedValue(undefined);
  getAppSettings.mockReset().mockResolvedValue({
    onboarding_completed: true,
    budget: { enabled: true, monthly_limit_nanodollars: 50_000_000_000 },
    recorder_adapter: "sqlite",
    voice_provider_model: "openai/gpt-realtime-2",
    tool_providers: { xai: false, x: false, search: false },
    permission_policy: {
      allowed_folders: [],
      denied_folders: [],
      allowed_url_hosts: [],
      denied_url_hosts: [],
      allow_all_urls: false,
    },
  });
  useCostStore.getState().reset();
});

describe("CostHeader", () => {
  it("shows this month's spend and the cap for an enabled budget", () => {
    act(() => useCostStore.getState().applySnapshot(snapshot({ sequence: 1 })));
    render(<CostHeader />);
    expect(screen.getByText(/今月/)).toHaveTextContent("今月: $16.00 / 上限 $32.00");
  });

  it("shows 上限なし for an explicitly unlimited budget (not a fabricated cap)", () => {
    act(() =>
      useCostStore.getState().applySnapshot(
        snapshot({ sequence: 1, enabled: false, limit_nanodollars: null, remaining_usd: null, used_usd: 5 }),
      ),
    );
    render(<CostHeader />);
    expect(screen.getByText(/今月/)).toHaveTextContent("今月: $5.00 / 上限なし");
  });

  it("shows an explicit unknown state on load failure (never a fabricated $0)", () => {
    act(() => useCostStore.getState().setLoadError("使用額を取得できませんでした。"));
    render(<CostHeader />);
    expect(screen.getByText(/取得できません/)).toBeInTheDocument();
    // It must NOT claim $0.00 spent.
    expect(screen.queryByText(/\$0\.00/)).not.toBeInTheDocument();
  });

  it("renders an over-budget stop alert + a raise control when over budget", () => {
    act(() =>
      useCostStore.getState().applySnapshot(
        snapshot({ sequence: 1, over_budget: true, used_usd: 32, remaining_usd: 0 }),
      ),
    );
    render(<CostHeader />);
    expect(screen.getByRole("alert")).toHaveTextContent(/停止/);
    expect(screen.getByRole("button", { name: /上限を引き上げる/ })).toBeInTheDocument();
  });

  it("raising the limit persists the new cap and re-fetches the snapshot", async () => {
    act(() =>
      useCostStore.getState().applySnapshot(
        snapshot({ sequence: 1, over_budget: true, used_usd: 32, remaining_usd: 0 }),
      ),
    );
    // The re-fetch after raising returns an under-budget snapshot (higher cap).
    getCostSnapshot.mockResolvedValue(
      snapshot({ sequence: 2, over_budget: false, limit_nanodollars: 50_000_000_000, used_usd: 32, remaining_usd: 18 }),
    );
    render(<CostHeader />);

    fireEvent.change(screen.getByLabelText(/新しい月額上限/), { target: { value: "50" } });
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: /上限を引き上げる/ }));
    });

    await waitFor(() => {
      expect(saveBudgetConfig).toHaveBeenCalledWith(true, 50);
      expect(getCostSnapshot).toHaveBeenCalled();
    });
    // The header reflects the re-fetched, no-longer-over-budget snapshot.
    await waitFor(() => expect(useCostStore.getState().snapshot?.over_budget).toBe(false));
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("does not report a raise failure when the cap was saved but the re-fetch failed", async () => {
    // saveBudget succeeds (cap persisted) but the display re-pull throws. The user
    // must NOT be told the raise failed (it didn't) — that would invite a confused
    // double-raise. The cap is saved; only the display refresh failed.
    act(() =>
      useCostStore.getState().applySnapshot(
        snapshot({ sequence: 1, over_budget: true, used_usd: 32, remaining_usd: 0 }),
      ),
    );
    saveBudgetConfig.mockResolvedValue(undefined); // persist succeeds
    getCostSnapshot.mockRejectedValue(new Error("ledger unavailable")); // re-pull fails
    render(<CostHeader />);

    fireEvent.change(screen.getByLabelText(/新しい月額上限/), { target: { value: "50" } });
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: /上限を引き上げる/ }));
    });

    await waitFor(() => expect(saveBudgetConfig).toHaveBeenCalledWith(true, 50));
    // Honest message: acknowledges the save, does not claim the raise failed.
    expect(screen.getByText(/保存しました/)).toBeInTheDocument();
    expect(screen.queryByText(/保存に失敗/)).not.toBeInTheDocument();
  });

  it("rejects an empty/invalid raise amount via the JS guard without calling the backend", async () => {
    // The number input's min/max blocks out-of-range values natively; an EMPTY
    // value passes native constraints, so it exercises the component's own guard
    // (parseFloat("") -> NaN -> rejected) — which must NOT reach the backend.
    act(() =>
      useCostStore.getState().applySnapshot(snapshot({ sequence: 1, over_budget: true })),
    );
    render(<CostHeader />);
    fireEvent.change(screen.getByLabelText(/新しい月額上限/), { target: { value: "" } });
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: /上限を引き上げる/ }));
    });
    expect(saveBudgetConfig).not.toHaveBeenCalled();
    expect(screen.getByText(/有効な金額/)).toBeInTheDocument();
  });
});
