// Wires the backend `session-status` channel into sessionStore for the
// lifetime of the component that mounts it (typically the app root).
//
// Pattern mirrors useActivityEvents (src/features/activity/useActivityEvents.ts):
//  - `active` guard handles the race where unmount fires before the async
//    `listen()` resolves.
//  - Subscription teardown on unmount via the returned cleanup function.

import { useEffect } from "react";
import type { UnlistenFn } from "@tauri-apps/api/event";

import { onSessionStatus } from "../../lib/tauri/ipc";
import { useSessionStore } from "./sessionStore";

export function useSessionEvents(): void {
  useEffect(() => {
    let active = true;
    let unlisten: UnlistenFn | undefined;

    onSessionStatus((event) => {
      useSessionStore.getState().setFromEvent(event);
    })
      .then((fn) => {
        if (active) {
          unlisten = fn;
        } else {
          // Unmounted before the promise resolved — tear down immediately.
          fn();
        }
      })
      .catch(() => {
        // listen() rejected — the session-status channel is not wired.  If
        // `active` is still true the component is mounted and the user is
        // waiting for a usable UI; surface the failure so the store moves to
        // 'error' and the button shows a retryable error rather than staying
        // stuck on 'idle' silently (or 'loading' if startSession already ran).
        if (active) {
          const store = useSessionStore.getState();
          // FAIL-CLOSED (cost-protection P1): if the listener fails while a session
          // is already running (status === "connected" | "loading" | "reconnecting"),
          // fire an idempotent stopSession so the backend is not left running
          // unbounded with no UI way to reach it. `reconnecting` (koe-byf) is a LIVE
          // session — the backend supervisor is actively re-opening (billable)
          // connections — so it MUST be force-stopped here too, else a dropped status
          // channel during a reconnect storm leaves the supervisor reconnecting
          // unbounded with the UI stuck on the listener error. setListenerError() is
          // called first so the store transitions to listenerFailed=true /
          // status="error", which unblocks stopSession() for the listenerFailed path.
          const priorStatus = store.status;
          store.setListenerError();
          if (
            priorStatus === "connected" ||
            priorStatus === "loading" ||
            priorStatus === "reconnecting"
          ) {
            // Best-effort — Rust stop_session is idempotent; ignore rejections
            // here since the UI is already showing the fatal listener error.
            void useSessionStore.getState().stopSession();
          }
        }
        // If !active the component unmounted before the promise resolved;
        // nothing to clean and no UI to update.
      });

    return () => {
      active = false;
      unlisten?.();
    };
  }, []);
}
