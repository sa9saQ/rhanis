// Cost store (koe-9xi) — holds the latest monthly cost snapshot for the live
// header. Both the pull (`get_cost_snapshot`) and the push (`cost-update` event)
// fold through `applySnapshot`, which keeps only the highest-`sequence` snapshot.
//
// Design notes (mirrors activityStore / sessionStore):
//  - The backend decides `over_budget` in u64; this store never recomputes it.
//  - The sequence guard prevents a stale lower-sequence snapshot from overwriting
//    a newer over-budget one (which would hide the stop UI — a fail-open display).
//  - FAIL-CLOSED display: on a load failure the store keeps `snapshot` as-is (or
//    null) and sets `loadError`; it never fabricates a $0 / unlimited snapshot
//    that would hide real spend. The UI shows last-known + an error, or an
//    explicit "unknown" when there is no snapshot yet.
//  - `set((s) => ...)` pattern throughout.

import { create } from "zustand";

import type { CostSnapshot } from "./types";

interface CostState {
  /** Latest snapshot, or null until the first pull/push resolves. */
  snapshot: CostSnapshot | null;
  /** Highest `sequence` applied; guards against stale snapshots. */
  lastSequence: number;
  /** Fixed message when a pull/refetch failed; null otherwise. */
  loadError: string | null;

  /** Fold in a snapshot (pull or push). Ignores a stale (<= lastSequence) one. */
  applySnapshot: (snapshot: CostSnapshot) => void;
  /** Record a load/refetch failure without fabricating a snapshot. null clears. */
  setLoadError: (message: string | null) => void;
  /** Reset to the empty state (used in tests + on teardown). */
  reset: () => void;
}

function initialState() {
  return {
    snapshot: null as CostSnapshot | null,
    // -1 so a backend whose sequence starts at 0 is not ignored.
    lastSequence: -1,
    loadError: null as string | null,
  };
}

export const useCostStore = create<CostState>((set) => ({
  ...initialState(),

  applySnapshot: (snapshot) =>
    set((s) => {
      // Drop a stale or duplicate snapshot (`<=`, not `<`, so a re-emitted
      // same-sequence one cannot overwrite). This is what stops an older
      // under-budget snapshot from hiding a newer over-budget one.
      if (snapshot.sequence <= s.lastSequence) {
        return s;
      }
      // A fresh authoritative value clears any prior load error.
      return { ...s, snapshot, lastSequence: snapshot.sequence, loadError: null };
    }),

  setLoadError: (message) =>
    // Keep the last-known snapshot (if any) — showing a recent real value beats
    // showing "unknown", and never fabricates $0. With no snapshot, the header
    // renders the explicit unknown state from `loadError`.
    set((s) => ({ ...s, loadError: message })),

  reset: () => set(() => initialState()),
}));
