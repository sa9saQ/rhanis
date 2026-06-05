import { beforeEach, describe, expect, it } from "vitest";

import { useCostStore } from "./costStore";
import type { CostSnapshot } from "./types";

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
  useCostStore.getState().reset();
});

describe("costStore", () => {
  it("starts empty (no fabricated $0)", () => {
    const s = useCostStore.getState();
    expect(s.snapshot).toBeNull();
    expect(s.loadError).toBeNull();
  });

  it("applySnapshot stores the snapshot and tracks its sequence", () => {
    useCostStore.getState().applySnapshot(snapshot({ sequence: 5 }));
    const s = useCostStore.getState();
    expect(s.snapshot?.sequence).toBe(5);
    expect(s.snapshot?.used_usd).toBe(16);
  });

  it("ignores a stale (<= lastSequence) snapshot so a newer over-budget state is not overwritten", () => {
    useCostStore.getState().applySnapshot(snapshot({ sequence: 5, over_budget: true, used_usd: 40 }));
    // An older (lower-sequence) under-budget snapshot must NOT clobber the newer
    // over-budget one (that would hide the stop UI — a fail-open display).
    useCostStore.getState().applySnapshot(snapshot({ sequence: 4, over_budget: false, used_usd: 10 }));
    const s = useCostStore.getState().snapshot;
    expect(s?.over_budget).toBe(true);
    expect(s?.used_usd).toBe(40);
  });

  it("ignores an equal-sequence snapshot (<=, not <)", () => {
    useCostStore.getState().applySnapshot(snapshot({ sequence: 7, used_usd: 16 }));
    useCostStore.getState().applySnapshot(snapshot({ sequence: 7, used_usd: 99 }));
    expect(useCostStore.getState().snapshot?.used_usd).toBe(16);
  });

  it("applies a strictly newer snapshot", () => {
    useCostStore.getState().applySnapshot(snapshot({ sequence: 5, used_usd: 16 }));
    useCostStore.getState().applySnapshot(snapshot({ sequence: 6, used_usd: 20 }));
    expect(useCostStore.getState().snapshot?.used_usd).toBe(20);
  });

  it("a fresh snapshot clears a prior loadError", () => {
    useCostStore.getState().setLoadError("取得できません");
    useCostStore.getState().applySnapshot(snapshot({ sequence: 2 }));
    expect(useCostStore.getState().loadError).toBeNull();
  });

  it("setLoadError records the error without fabricating a snapshot", () => {
    useCostStore.getState().setLoadError("取得できません");
    const s = useCostStore.getState();
    expect(s.loadError).toBe("取得できません");
    expect(s.snapshot).toBeNull();
  });

  it("setLoadError does not blank a known snapshot (keep last-known, not unknown)", () => {
    useCostStore.getState().applySnapshot(snapshot({ sequence: 3, used_usd: 12 }));
    useCostStore.getState().setLoadError("再取得に失敗");
    const s = useCostStore.getState();
    expect(s.snapshot?.used_usd).toBe(12);
    expect(s.loadError).toBe("再取得に失敗");
  });

  it("reset clears everything", () => {
    useCostStore.getState().applySnapshot(snapshot({ sequence: 9 }));
    useCostStore.getState().setLoadError("x");
    useCostStore.getState().reset();
    const s = useCostStore.getState();
    expect(s.snapshot).toBeNull();
    expect(s.loadError).toBeNull();
    // After reset, a sequence-0 snapshot must still apply (lastSequence guard reset).
    useCostStore.getState().applySnapshot(snapshot({ sequence: 0 }));
    expect(useCostStore.getState().snapshot?.sequence).toBe(0);
  });
});
