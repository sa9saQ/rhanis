// Wires the backend event streams into the activity store for the lifetime of
// the component that mounts it (typically the app root). Subscriptions are
// torn down on unmount; an unmount that happens before the async `listen`
// handles resolve is handled via the `active` guard.

import { useEffect } from "react";
import type { UnlistenFn } from "@tauri-apps/api/event";

import {
  onApprovalRequired,
  onSessionStatus,
  onThinkingEvent,
  onToolEvent,
} from "../../lib/tauri/ipc";
import { useActivityStore } from "./activityStore";

export function useActivityEvents(): void {
  useEffect(() => {
    let active = true;
    const unlistens: UnlistenFn[] = [];
    const store = useActivityStore.getState();

    // Subscribe each channel independently: if one `listen()` rejects (webview
    // teardown), the others that already resolved are still tracked and cleaned
    // up — a single Promise.all reject would otherwise strand them.
    const track = (fn: UnlistenFn) => {
      if (active) {
        unlistens.push(fn);
      } else {
        fn(); // unmounted before this resolved — tear it down now
      }
    };
    const subscribe = <T>(
      on: (handler: (payload: T) => void) => Promise<UnlistenFn>,
      handler: (payload: T) => void,
    ) => {
      on(handler)
        .then(track)
        .catch(() => {
          // listen() can reject if the webview is tearing down; nothing to clean.
        });
    };

    subscribe(onToolEvent, (event) => store.ingestToolEvent(event));
    subscribe(onThinkingEvent, (event) => store.ingestThinkingEvent(event));
    subscribe(onApprovalRequired, (request) => store.enqueueApproval(request));
    subscribe(onSessionStatus, (status) => store.setSessionStatus(status));

    return () => {
      active = false;
      unlistens.forEach((fn) => fn());
    };
  }, []);
}
