// The operator console — Rhanis's primary differentiator: a single panel that
// shows, at a glance, what the assistant is doing right now. Status indicator,
// the live actions (with elapsed time + progress), and the recent event log.

import { useEffect, useState } from "react";
import { useShallow } from "zustand/react/shallow";

import {
  selectActiveActions,
  selectDisplayStatus,
  selectRecentProviderErrors,
  selectRecentThinking,
  useActivityStore,
} from "./activityStore";
import type {
  ActionState,
  DisplayStatus,
  ProviderErrorEvent,
  ThinkingEvent,
  ToolEvent,
} from "./types";
import "./ActivityLog.css";

/** How many recent disclosures the thinking trace shows at once. */
const THINKING_VISIBLE = 3;

/** How many recent provider/server errors the error strip shows at once (rhanis-nal). */
const PROVIDER_ERRORS_VISIBLE = 3;

/** Japanese label + dot tone for each derived status. */
const STATUS_META: Record<DisplayStatus, { label: string; tone: string }> = {
  idle: { label: "待機", tone: "idle" },
  connecting: { label: "準備", tone: "connecting" },
  reconnecting: { label: "再接続中", tone: "reconnecting" },
  conversing: { label: "会話", tone: "conversing" },
  working: { label: "作業", tone: "working" },
  error: { label: "エラー", tone: "error" },
};

/** A clock that ticks once a second to refresh elapsed times — only while needed. */
function useNow(active: boolean): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (!active) return;
    const id = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(id);
  }, [active]);
  return now;
}

function elapsedLabel(ms: number): string {
  const s = Math.max(0, Math.floor(ms / 1000));
  if (s < 60) return `${s}s`;
  return `${Math.floor(s / 60)}m${(s % 60).toString().padStart(2, "0")}s`;
}

function LiveAction({ action, now }: { action: ActionState; now: number }) {
  const pct = action.progress != null ? Math.round(action.progress * 100) : null;
  return (
    <li className="rhanis-live-action">
      <span className="rhanis-spinner" aria-hidden />
      <span className="rhanis-live-tool">{action.tool}</span>
      <span className="rhanis-live-summary">{action.displaySummary}</span>
      {pct != null && (
        <span className="rhanis-live-progress" role="progressbar" aria-valuenow={pct}>
          {pct}%
        </span>
      )}
      <span className="rhanis-live-elapsed">{elapsedLabel(now - action.startedAt)}</span>
    </li>
  );
}

/**
 * One thinking disclosure (glass-box M1, rhanis-sua.1): what Rhanis is about to do and
 * the verifiable act (tool / source). Shows the redacted `plan` + the checkable
 * tool/source — never raw chain-of-thought, which the backend does not send.
 */
function ThinkingRow({ thought }: { thought: ThinkingEvent }) {
  return (
    <li className="rhanis-thinking-row">
      <span className="rhanis-thinking-glyph" aria-hidden>
        💭
      </span>
      <span className="rhanis-thinking-plan">{thought.plan}</span>
      {thought.tool && <span className="rhanis-thinking-tool">{thought.tool}</span>}
      {thought.source && <span className="rhanis-thinking-source">{thought.source}</span>}
    </li>
  );
}

/**
 * One non-benign provider/server error (rhanis-nal) — e.g. a rejected
 * `session.update`, after which tools / 記録 silently stop working. The backend
 * pre-sanitizes + caps `code` / `message`, so they render as plain text.
 */
function ProviderErrorRow({ error }: { error: ProviderErrorEvent }) {
  return (
    <li className="rhanis-provider-error-row">
      <span className="rhanis-provider-error-glyph" aria-hidden>
        ⚠
      </span>
      <span className="rhanis-provider-error-text">
        サーバーエラー{error.code ? ` (${error.code})` : ""}: {error.message}
      </span>
    </li>
  );
}

const PHASE_GLYPH: Record<ToolEvent["phase"], string> = {
  start: "▶",
  progress: "…",
  done: "✓",
  error: "✕",
};

function LogRow({ event }: { event: ToolEvent }) {
  return (
    <li className={`rhanis-log-row rhanis-phase-${event.phase}`}>
      <span className="rhanis-log-glyph" aria-hidden>
        {PHASE_GLYPH[event.phase]}
      </span>
      <span className="rhanis-log-tool">{event.tool}</span>
      <span className="rhanis-log-summary">{event.displaySummary}</span>
      {/* The backend's pre-redacted WHY ("tool not implemented", "declined by
          operator", caution note…) — without it an error row shows only THAT
          something failed, not why (rhanis-r2o R-C). */}
      {event.detail && <span className="rhanis-log-detail">{event.detail}</span>}
    </li>
  );
}

export function ActivityLog({ className }: { className?: string } = {}) {
  const status = useActivityStore(selectDisplayStatus);
  // `selectActiveActions` builds a fresh array; `useShallow` compares its
  // contents so the component doesn't re-render (and loop) every tick.
  const active = useActivityStore(useShallow(selectActiveActions));
  const thinking = useActivityStore(useShallow(selectRecentThinking));
  const providerErrors = useActivityStore(useShallow(selectRecentProviderErrors));
  const events = useActivityStore((s) => s.events);
  const pendingApprovals = useActivityStore((s) => s.approvalQueue.length);
  const lastError = useActivityStore((s) => s.lastError);

  const now = useNow(active.length > 0);
  const meta = STATUS_META[status];
  // Newest first in the visible log.
  const recent = [...events].reverse();

  return (
    <section
      className={className ? `rhanis-console ${className}` : "rhanis-console"}
      aria-label="アクティビティ"
    >
      <header className="rhanis-console-head">
        <span className={`rhanis-status-dot rhanis-tone-${meta.tone}`} aria-hidden />
        <span className="rhanis-status-label">{meta.label}</span>
        {pendingApprovals > 0 && (
          <span className="rhanis-approval-badge">承認待ち {pendingApprovals}</span>
        )}
      </header>

      {/* The live monthly cost header (rhanis-9xi) moved to the console sidebar
          foot (rhanis-ios.1, ConsoleLayout) — the brief pins cost at the bottom
          of the sidebar, always visible next to 設定. */}
      {status === "error" && lastError && (
        <p className="rhanis-error-line" role="alert">
          {lastError}
        </p>
      )}

      {/* Provider/server errors (rhanis-nal): a rejected session.update etc. —
          surfaced WITHOUT ending the session (session-status error is the
          terminal contract). Always mounted for the same assistive-tech
          reliability as the thinking window below. */}
      <ul className="rhanis-provider-errors" aria-label="サーバーエラー" aria-live="polite">
        {providerErrors.slice(0, PROVIDER_ERRORS_VISIBLE).map((e) => (
          <ProviderErrorRow key={e.eventId} error={e} />
        ))}
      </ul>

      {/* Thinking window (glass-box M1, rhanis-sua.1): what Rhanis is about to do,
          disclosed BEFORE the tool runs so a silent pause reads as deliberation.
          The live region is ALWAYS mounted (empty when idle) — like rhanis-live
          below — so assistive tech registers it first and reliably announces the
          first disclosure (the most important one, in the 300-700ms window); a
          region inserted together with its content is announced unreliably. */}
      <ul className="rhanis-thinking" aria-label="考えていること" aria-live="polite">
        {thinking.slice(0, THINKING_VISIBLE).map((t) => (
          <ThinkingRow key={t.eventId} thought={t} />
        ))}
      </ul>

      <div className="rhanis-live" aria-live="polite">
        {active.length === 0 ? (
          <p className="rhanis-live-empty">いまは静かです</p>
        ) : (
          <ul className="rhanis-live-list">
            {active.map((a) => (
              <LiveAction key={a.actionId} action={a} now={now} />
            ))}
          </ul>
        )}
      </div>

      <ol className="rhanis-log" aria-label="直近の動作">
        {recent.length === 0 ? (
          <li className="rhanis-log-empty">まだ記録はありません</li>
        ) : (
          recent.map((e) => <LogRow key={e.eventId} event={e} />)
        )}
      </ol>
    </section>
  );
}
