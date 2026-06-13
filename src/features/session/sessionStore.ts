// Session store — owns the start/stop lifecycle and reflects the live
// connection state that arrives from the backend `session-status` channel.
//
// Design notes (mirroring activityStore / settingsStore patterns):
//  - `status` is the canonical state machine; "loading" covers both "connecting"
//    and the brief window between `startSession()` invoke and the first
//    `session-status` event arriving from Rust.
//  - The backend is the authority on connection state; the store merely reflects
//    it via `setFromEvent()` called by the subscriber in `useSessionEvents`.
//  - `error` is sticky: it persists until a new non-error status or a fresh
//    start clears it — so the user can always see why the last session failed.
//  - In-flight guards (`inFlight`) prevent concurrent start/stop calls. The
//    button disables itself while a call is in flight.
//  - `set((s) => ...)` pattern throughout (matches the rest of the stores).

import { create } from "zustand";

import {
  startSession as ipcStartSession,
  stopSession as ipcStopSession,
} from "../../lib/tauri/ipc";
import type { SessionConnState, SessionStatusEvent } from "../activity/types";

/**
 * The session lifecycle status as seen by the UI. `reconnecting` (rhanis-byf) is a
 * live, STOPPABLE state (the supervisor is retrying a recoverable drop) — distinct
 * from the initial `loading` (which disables the button while first connecting).
 */
export type SessionStatus = "idle" | "loading" | "connected" | "reconnecting" | "error";

interface SessionState {
  status: SessionStatus;
  /** Sticky error message; cleared on a fresh start or a new non-error status. */
  error: string | null;
  /** Highest `sequence` seen on `session-status`; guards against stale events. */
  lastSequence: number;
  /**
   * Set by useSessionEvents when the Tauri `listen()` call itself rejects.
   * When true, the session-status channel is not wired: starting a session
   * would leave the UI stuck on "loading" with no way to receive a stop event.
   * The UI should show this as a fatal error and disable the start button.
   */
  listenerFailed: boolean;

  /** Invoke `start_session` on the backend. No-op if already loading/connected. */
  startSession: () => Promise<void>;
  /** Invoke `stop_session` on the backend. No-op if idle. */
  stopSession: () => Promise<void>;
  /**
   * Called by the `session-status` event subscriber (useSessionEvents hook)
   * with each incoming status from the backend. Ignores stale sequences.
   */
  setFromEvent: (event: SessionStatusEvent) => void;
  /**
   * Called by useSessionEvents when the Tauri listen() call rejects.
   * Transitions the store to an error state so the UI shows the failure
   * and the start button is NOT disabled (it stays retryable).
   */
  setListenerError: () => void;
  /** Reset to idle (used in tests). */
  reset: () => void;
}

function mapConnState(state: SessionConnState): SessionStatus {
  switch (state) {
    case "idle":
      return "idle";
    case "connecting":
      return "loading";
    case "connected":
      return "connected";
    // rhanis-byf: a distinct status (not "loading") so the UI keeps the session
    // stoppable while recovering and can show "再接続中" rather than "準備中…".
    case "reconnecting":
      return "reconnecting";
    case "error":
      return "error";
    default:
      return "idle";
  }
}

function initialState() {
  return {
    status: "idle" as SessionStatus,
    error: null as string | null,
    // -1 so a backend whose status sequence starts at 0 is not ignored.
    lastSequence: -1,
    listenerFailed: false,
  };
}

// Separate in-flight guards — not part of Zustand state because they must
// survive re-renders without triggering them.
//
// startInFlight: set for the duration of ipcStartSession(). Guards against
//   concurrent start calls (double-start prevention).
// stopInFlight: set for the duration of ipcStopSession(). Guards against
//   concurrent stop calls (double-stop prevention).
//
// Critically, stopSession does NOT check startInFlight. This is intentional:
// if listen() rejects while ipcStartSession() is awaiting, useSessionEvents
// calls setListenerError() (→ status "error", listenerFailed=true) and then
// stopSession(). With a shared guard, that stop would return early and leave
// the backend session running unbounded (cost risk). Separating the guards
// means the forced-stop path always reaches ipcStopSession().
let startInFlight = false;
let stopInFlight = false;

export const useSessionStore = create<SessionState>((set, get) => ({
  ...initialState(),

  startSession: async () => {
    const { status, listenerFailed } = get();
    // reconnecting is already an active session (rhanis-byf) — start is a no-op, like
    // connected. (stopSession below does NOT bail on reconnecting, so it stays
    // stoppable.)
    if (
      startInFlight ||
      status === "loading" ||
      status === "connected" ||
      status === "reconnecting"
    )
      return;
    // FAIL-CLOSED: if the event channel could not be opened, starting a session
    // would leave the backend running with no way to receive its status events —
    // the UI would be stuck on "loading" with no stop path.  Block the start and
    // keep the listener-error state visible so the user knows to restart the app.
    if (listenerFailed) {
      set((s) => ({
        ...s,
        status: "error" as const,
        error:
          "音声状態の購読に失敗したため開始できません。アプリを再起動してください。",
      }));
      return;
    }
    startInFlight = true;
    // Transition to loading immediately so the button shows a spinner before
    // the first `session-status` event arrives from the backend.
    set((s) => ({ ...s, status: "loading", error: null }));
    try {
      await ipcStartSession();
      // The backend will emit `session-status` { state: "connecting" } then
      // { state: "connected" }. Those arrive via setFromEvent() and drive the
      // store from here on — we don't set "connected" here directly.
    } catch {
      // Invoke itself failed (e.g. budget exceeded, no API key, onboarding
      // incomplete). The backend may or may not have emitted an error status
      // before the invoke rejected (race). If setFromEvent() already drove the
      // store to 'error' with a specific reason, preserve it — overwriting it
      // with a generic message would discard the real cause.
      set((s) =>
        s.status === "error" && s.error
          ? s // backend-emitted reason already in place; keep it
          : {
              ...s,
              status: "error" as const,
              error: "セッションを開始できませんでした。設定を確認してください。",
            },
      );
    } finally {
      startInFlight = false;
    }
  },

  stopSession: async () => {
    const { status, listenerFailed } = get();
    // Double-stop prevention: if a stop is already in-flight, bail out.
    // Note: we do NOT check startInFlight here. If listen() rejects while
    // ipcStartSession() is still awaiting (startInFlight=true), setListenerError()
    // drives status to "error"/listenerFailed=true and then stopSession() must
    // reach ipcStopSession() to prevent an orphaned backend session (cost P1).
    // That path clears the status guards below (not "idle", not "loading",
    // listenerFailed=true), so only the stop-specific double-stop guard applies.
    if (stopInFlight) return;
    // STOPPABLE during "loading" (rhanis-5fs): the previous guard also bailed on
    // "loading" to avoid racing a connecting backend, but that left a hung
    // "準備中…" with no escape (symptom 4). Stopping is safe now —
    // run_session_supervised races connect() against the master stop via
    // tokio::select! (session_manager.rs ~1095): a stop mid-connect abandons the
    // attempt and finalizes idle (emitting a "session-status" idle that drives
    // this store back to idle), so there is no orphaned connecting session.
    // ipcStopSession is idempotent and generation-guarded on the Rust side.
    //
    // idle: nothing to stop — bail.
    // error (non-listener): the backend read loop has already exited and cleared
    // the session slot; stop_session would hit an already-idle slot and, if it
    // rejects for any other reason, would clobber the sticky error message with a
    // second misleading error overlay.
    // EXCEPTION — listenerFailed error: the event channel is down but the backend
    // session may still be running (listener failed during or after start).  We
    // MUST allow stopSession here (idempotent on the Rust side) so the user can
    // prevent an orphaned, unbounded backend session (cost-protection P1).
    if (status === "idle") return;
    if (status === "error" && !listenerFailed) return;
    stopInFlight = true;
    try {
      await ipcStopSession();
      // The backend emits `session-status` { state: "idle" } via the read-loop
      // shutdown. setFromEvent() will drive the store to "idle". In the rare
      // case the event races the invoke (or the event was already emitted before
      // this returns), the sequence guard in setFromEvent keeps things consistent.
    } catch {
      // stop_session is documented as idempotent; errors here are unlikely
      // but guard anyway — don't leave the UI stuck on a non-idle status.
      set((s) => ({
        ...s,
        status: "error",
        error: "セッションの停止に失敗しました。",
      }));
    } finally {
      stopInFlight = false;
    }
  },

  setFromEvent: (event: SessionStatusEvent) =>
    set((s) => {
      // Ignore stale events — same guard as activityStore.setSessionStatus.
      if (event.sequence <= s.lastSequence) return s;
      const status = mapConnState(event.state);
      return {
        ...s,
        status,
        // Preserve a sticky error if a genuinely newer error arrives; clear
        // it on any non-error state.
        error: status === "error" ? (event.error ?? "不明なエラーが発生しました") : null,
        lastSequence: event.sequence,
      };
    }),

  setListenerError: () =>
    set((s) => ({
      ...s,
      status: "error",
      listenerFailed: true,
      // i18n-ready literal: the IPC event channel itself could not be opened.
      error:
        "イベントチャンネルを開けませんでした。アプリを再起動してください。",
    })),

  reset: () => {
    startInFlight = false;
    stopInFlight = false;
    set(() => initialState());
  },
}));
