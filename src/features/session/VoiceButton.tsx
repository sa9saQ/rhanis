// VoiceButton — the primary control for starting and stopping a Realtime
// session. It reads from sessionStore and invokes start/stop accordingly.
//
// Accessibility:
//  - Single <button> with a dynamic aria-label: "セッションを開始" / "セッションを停止".
//  - aria-pressed="true" while connected (the button acts as a toggle).
//  - aria-busy="true" while loading (backend is connecting).
//  - role="alert" error paragraph below the button for screen-reader
//    announcement of failures.
//  - All interactive states are keyboard-reachable (button element, no div hack).
//
// Visual style follows the existing koe design:
//  - No Inter, no purple/blue gradient (anti-AI-smell).
//  - Tokens from App.css (:root) — --accent, --tone-error, --ease-spring, etc.
//  - Button radii differ from the panel (10px vs panel's 14px) on purpose.

import { useSessionStore } from "./sessionStore";
import type { SessionStatus } from "./sessionStore";
import "./VoiceButton.css";

// In the listenerFailed error state, STATUS_META["error"] still renders
// (label "再試行") but VoiceButton overrides the click to call stopSession,
// not startSession, so the user always has a usable "stop" path.

/** Text and meta for each status. */
const STATUS_META: Record<
  SessionStatus,
  { label: string; ariaLabel: string; tone: string }
> = {
  idle: {
    label: "話す",
    ariaLabel: "セッションを開始",
    tone: "idle",
  },
  loading: {
    label: "準備中…",
    ariaLabel: "準備中",
    tone: "loading",
  },
  connected: {
    label: "停止",
    ariaLabel: "セッションを停止",
    tone: "connected",
  },
  // koe-byf: a live, STOPPABLE state — the button shows a spinner but stays enabled
  // and acts as "stop" so the user is never trapped during a long reconnect.
  reconnecting: {
    label: "停止",
    ariaLabel: "セッションを停止（再接続中）",
    tone: "loading",
  },
  error: {
    label: "再試行",
    ariaLabel: "セッションを開始（再試行）",
    tone: "error",
  },
};

export function VoiceButton() {
  const status = useSessionStore((s) => s.status);
  const error = useSessionStore((s) => s.error);
  const listenerFailed = useSessionStore((s) => s.listenerFailed);
  const startSession = useSessionStore((s) => s.startSession);
  const stopSession = useSessionStore((s) => s.stopSession);

  // In the listener-failed error state, override the button label/aria to "停止"
  // so the user understands the action (stop any running session, not start).
  const metaBase = STATUS_META[status];
  const meta =
    status === "error" && listenerFailed
      ? {
          ...metaBase,
          label: "停止",
          ariaLabel: "セッションを停止（リスナーエラー）",
        }
      : metaBase;
  // `loading` DISABLES the button (first connect in flight). `reconnecting` (koe-byf)
  // shows the same spinner / busy state but stays ENABLED so the user can stop a
  // recovering session — so split "show a spinner" (busy) from "is disabled".
  const isLoading = status === "loading";
  const isReconnecting = status === "reconnecting";
  const busy = isLoading || isReconnecting;

  async function handleClick() {
    if (status === "connected" || status === "reconnecting") {
      await stopSession();
    } else if (status === "error") {
      // FAIL-CLOSED: if the listener channel itself failed, the "retry" action
      // must attempt to stop any running backend session — NOT start a new one
      // (which would leave the backend unbounded with no event channel).
      if (listenerFailed) {
        await stopSession();
      } else {
        await startSession();
      }
    } else if (status === "idle") {
      await startSession();
    }
    // loading → no-op (button is disabled)
  }

  return (
    <div className="koe-voice-button-wrap">
      <button
        type="button"
        className={`koe-voice-btn koe-voice-tone-${meta.tone}`}
        aria-label={meta.ariaLabel}
        aria-pressed={status === "connected" || status === "reconnecting"}
        aria-busy={busy ? "true" : undefined}
        disabled={isLoading}
        onClick={() => void handleClick()}
      >
        {busy && (
          <span className="koe-voice-spinner" aria-hidden />
        )}
        <span className="koe-voice-btn-label">{meta.label}</span>
      </button>

      {status === "error" && error && (
        <p className="koe-voice-error" role="alert">
          {error}
        </p>
      )}
    </div>
  );
}
