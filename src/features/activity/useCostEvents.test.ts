import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { CostSnapshot } from "./types";

// Capture the cost-update handler and hand back unlisten spies; make the pull
// (getCostSnapshot) configurable per test.
let costHandler: ((s: CostSnapshot) => void) | undefined;
const unlistenSpies: Array<ReturnType<typeof vi.fn>> = [];
const getCostSnapshotMock = vi.fn();
// When `deferSubscribe` is true, onCostUpdate returns a promise that stays pending
// until `resolveSubscribe()` is called — used to assert the pull only fires AFTER
// the subscription is established (the Codex R-C fail-open race fix).
let deferSubscribe = false;
let resolveSubscribe: (() => void) | undefined;

vi.mock("../../lib/tauri/ipc", () => ({
  onCostUpdate: (h: (s: CostSnapshot) => void) => {
    costHandler = h;
    const spy = vi.fn();
    unlistenSpies.push(spy);
    if (deferSubscribe) {
      return new Promise<typeof spy>((res) => {
        resolveSubscribe = () => res(spy);
      });
    }
    return Promise.resolve(spy);
  },
  getCostSnapshot: () => getCostSnapshotMock(),
}));

import { useCostStore } from "./costStore";
import { useCostEvents } from "./useCostEvents";

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
  costHandler = undefined;
  unlistenSpies.length = 0;
  deferSubscribe = false;
  resolveSubscribe = undefined;
  getCostSnapshotMock.mockReset();
  getCostSnapshotMock.mockResolvedValue(snapshot({ sequence: 1 }));
  useCostStore.getState().reset();
});

describe("useCostEvents", () => {
  it("subscribes to cost-update and pulls the initial snapshot", async () => {
    renderHook(() => useCostEvents());
    await waitFor(() => {
      expect(costHandler).toBeTypeOf("function");
      expect(getCostSnapshotMock).toHaveBeenCalledTimes(1);
    });
  });

  it("applies the pulled snapshot to the store", async () => {
    getCostSnapshotMock.mockResolvedValue(snapshot({ sequence: 2, used_usd: 21 }));
    renderHook(() => useCostEvents());
    await waitFor(() => expect(useCostStore.getState().snapshot?.used_usd).toBe(21));
  });

  it("applies a pushed snapshot through the handler", async () => {
    renderHook(() => useCostEvents());
    await waitFor(() => expect(costHandler).toBeTypeOf("function"));
    act(() => costHandler?.(snapshot({ sequence: 9, over_budget: true, used_usd: 40 })));
    const s = useCostStore.getState().snapshot;
    expect(s?.over_budget).toBe(true);
    expect(s?.used_usd).toBe(40);
  });

  it("sets a fail-closed loadError when the pull rejects (no fabricated $0)", async () => {
    getCostSnapshotMock.mockRejectedValue(new Error("ledger unavailable"));
    renderHook(() => useCostEvents());
    await waitFor(() => expect(useCostStore.getState().loadError).toBeTruthy());
    expect(useCostStore.getState().snapshot).toBeNull();
  });

  it("does not pull until the cost-update subscription is established (no lost over-budget race)", async () => {
    // Fail-open race fix (Codex R-C): the pull must not start until listen() has
    // resolved, so an over-budget cost-update emitted right after subscribe is not
    // lost to a still-unregistered listener.
    deferSubscribe = true;
    renderHook(() => useCostEvents());
    // Subscription promise is still pending → no pull yet.
    await Promise.resolve();
    await Promise.resolve();
    expect(getCostSnapshotMock).not.toHaveBeenCalled();
    // Once the listener is live, the pull runs.
    await act(async () => {
      resolveSubscribe?.();
    });
    await waitFor(() => expect(getCostSnapshotMock).toHaveBeenCalledTimes(1));
  });

  it("unlistens the cost channel on unmount", async () => {
    const { unmount } = renderHook(() => useCostEvents());
    await waitFor(() => expect(unlistenSpies).toHaveLength(1));
    unmount();
    await waitFor(() => expect(unlistenSpies[0]).toHaveBeenCalledTimes(1));
  });
});
