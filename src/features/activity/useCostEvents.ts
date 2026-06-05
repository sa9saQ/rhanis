// Wires the cost snapshot into costStore for the lifetime of the mounting
// component (the app root): subscribes to live `cost-update` pushes AND pulls the
// authoritative current value once on mount. Both fold through the store's
// sequence-guarded `applySnapshot`, so the pull/push race resolves to whichever
// is newer.
//
// Pattern mirrors useActivityEvents / useSessionEvents: an `active` guard handles
// the unmount-before-`listen()`-resolves race, and the subscription is torn down
// on unmount.

import { useEffect } from "react";
import type { UnlistenFn } from "@tauri-apps/api/event";

import { getCostSnapshot, onCostUpdate } from "../../lib/tauri/ipc";
import { useCostStore } from "./costStore";

export function useCostEvents(): void {
  useEffect(() => {
    let active = true;
    let unlisten: UnlistenFn | undefined;

    // Pull the authoritative current value. FAIL-CLOSED: on error show an explicit
    // "unknown" (never a fabricated $0 that would hide real spend).
    const pull = () => {
      getCostSnapshot()
        .then((snapshot) => {
          if (active) useCostStore.getState().applySnapshot(snapshot);
        })
        .catch(() => {
          if (active) {
            useCostStore.getState().setLoadError("使用額を取得できませんでした。");
          }
        });
    };

    // Subscribe to live pushes, then pull ONLY ONCE THE SUBSCRIPTION IS ESTABLISHED
    // (or has failed). Pulling after the listener is live closes a fail-open race
    // (Codex R-C): if we pulled concurrently, an over-budget `cost-update` emitted
    // before `listen()` registered would be LOST (Tauri does not buffer events for a
    // late listener), leaving a stale under-budget header that hides the stop UI.
    // Once the listener is live, the pull reads the authoritative ledger (which
    // already includes all prior spend) and the store's sequence guard reconciles
    // any pull/push overlap, so no over-budget snapshot can be dropped.
    onCostUpdate((snapshot) => {
      useCostStore.getState().applySnapshot(snapshot);
    })
      .then((fn) => {
        if (!active) {
          fn(); // unmounted before the promise resolved — tear it down now
          return;
        }
        unlisten = fn;
        pull();
      })
      .catch(() => {
        // listen() rejected (e.g. webview teardown). Still pull a one-shot value so
        // the header isn't blank; we just won't get live updates.
        if (active) pull();
      });

    return () => {
      active = false;
      unlisten?.();
    };
  }, []);
}
