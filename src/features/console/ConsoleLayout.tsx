// ConsoleLayout — the glass-box console shell (koe-ios.1).
//
// Approved layout (docs/design/2026-06-10-glassbox-console-design-brief.md):
// a collapsible left sidebar (brand / destinations / cost + settings at the
// bottom) and the right main column: status-aware greeting (the conversation
// area) → live activity panel (the hero) → the voice orb.
//
// Honesty constraints (wiring.md — no dead UI, no fake affordances):
//  - No conversation-transcript stream reaches the frontend yet (history UI =
//    koe-sh6), so the conversation area renders the status-aware greeting only
//    — never a fabricated transcript.
//  - Sidebar destinations whose features ship in separate issues (検索/履歴 =
//    koe-sh6, オートメーション = koe-bu1, 手足ツール = koe-eal / koe-v5i,
//    プロジェクト / タスクボード = post-E2E backlog) render as a non-interactive
//    "近日追加" list — visible structure from the approved brief, not
//    fake-clickable buttons.

import { useState } from "react";

import { ActivityLog } from "../activity/ActivityLog";
import { ApprovalModal } from "../activity/ApprovalModal";
import { CostHeader } from "../activity/CostHeader";
import { DevMockEmitter } from "../activity/DevMockEmitter";
import { useActivityEvents } from "../activity/useActivityEvents";
import { useCostEvents } from "../activity/useCostEvents";
import { useCostStore } from "../activity/costStore";
import { VoiceButton } from "../session/VoiceButton";
import { useSessionEvents } from "../session/useSessionEvents";
import { useSessionStore } from "../session/sessionStore";
import type { SessionStatus } from "../session/sessionStore";
import { SettingsPanel } from "../settings/SettingsPanel";
import "./ConsoleLayout.css";

/**
 * Status-aware greeting — the voice-first conversation anchor. Spoken-style,
 * one line, no jargon. `error` must NOT look like a calm idle screen (an h1
 * that says 今日は何をしましょう？ over a dead session is a false-normal
 * display); the specific error text + retry stay on VoiceButton's role=alert.
 */
const GREETING: Record<SessionStatus, string> = {
  idle: "今日は何をしましょう？",
  loading: "準備しています…",
  connected: "どうぞ、話しかけてください",
  reconnecting: "再接続しています…",
  error: "接続に問題があります",
};

/** Planned sidebar destinations from the approved brief (see header comment).
 * 新しい会話 is NOT here — starting a conversation works today, so it renders
 * as a real button wired to startSession instead of a planned-list entry. */
const PLANNED_NAV: ReadonlyArray<{ glyph: string; label: string }> = [
  { glyph: "🔍", label: "検索" },
  { glyph: "📁", label: "プロジェクト" },
  { glyph: "⚡", label: "オートメーション" },
  { glyph: "🧩", label: "手足ツール" },
  { glyph: "📋", label: "タスクボード" },
];

export function ConsoleLayout() {
  // Subscribe to the backend tool-event / approval / status streams for the
  // app's lifetime.
  useActivityEvents();
  // Subscribe to the backend session-status stream; drives sessionStore.
  useSessionEvents();
  // Pull + subscribe to the live monthly cost snapshot; drives costStore (koe-9xi).
  useCostEvents();

  const status = useSessionStore((s) => s.status);
  const startSession = useSessionStore((s) => s.startSession);
  const overBudget = useCostStore((s) => s.snapshot?.over_budget ?? false);

  const [showSettings, setShowSettings] = useState(false);
  const [sidebarOpen, setSidebarOpen] = useState(true);

  // The over-budget stop notice + raise control live in the sidebar's
  // CostHeader. They must never sit behind a collapsed sidebar (fail-closed
  // UX), so visibility is DERIVED — not synced via an effect, which would
  // only fire on the false→true transition and let a manual collapse hide
  // the notice for the rest of the over-budget episode (R-B finding). The
  // toggle is disabled while over budget so it doesn't look broken.
  const sidebarVisible = sidebarOpen || overBudget;

  return (
    <div className="koe-shell">
      {sidebarVisible && (
        <aside className="koe-sidebar" id="koe-sidebar" aria-label="サイドバー">
          <div className="koe-side-brand">koe</div>

          {/* The one destination that works today: starting a conversation.
              Wired to the same store action as the voice orb; disabled unless
              idle (the store also guards re-entry) — and while over budget,
              where the backend would reject the start anyway (R-C finding:
              don't show 開始 next to the 上限を引き上げて stop notice). */}
          <button
            type="button"
            className="koe-btn koe-btn-side"
            disabled={status !== "idle" || overBudget}
            onClick={() => void startSession()}
          >
            <span className="koe-side-glyph" aria-hidden>
              💬
            </span>
            <span>新しい会話</span>
          </button>

          {/* Deliberately NOT a <nav> landmark: these are upcoming
              destinations, not working navigation (honest semantics). The
              list becomes a real <nav> when the first destination ships. */}
          <div className="koe-side-nav">
            <p className="koe-side-caption" id="koe-side-planned-caption">
              近日追加
            </p>
            <ul
              className="koe-side-planned"
              aria-labelledby="koe-side-planned-caption"
            >
              {PLANNED_NAV.map((item) => (
                <li key={item.label} className="koe-side-item">
                  <span className="koe-side-glyph" aria-hidden>
                    {item.glyph}
                  </span>
                  <span>{item.label}</span>
                </li>
              ))}
            </ul>
          </div>

          <div className="koe-side-foot">
            {/* Live monthly cost + over-budget stop / raise control (koe-9xi).
                The brief's 残高+時間併記 is the M4 managed-credit form (koe-3x6);
                M1 BYOK shows the real thing we have: monthly spend vs cap. */}
            <CostHeader />
            <button
              type="button"
              className="koe-btn koe-btn-side"
              aria-expanded={showSettings}
              aria-controls={showSettings ? "koe-settings-area" : undefined}
              onClick={() => setShowSettings((v) => !v)}
            >
              <span className="koe-side-glyph" aria-hidden>
                ⚙
              </span>
              <span>設定</span>
            </button>
          </div>
        </aside>
      )}

      <main className="koe-main">
        <div className="koe-main-bar">
          <button
            type="button"
            className="koe-btn koe-btn-bar"
            aria-label="サイドバーを開閉"
            aria-expanded={sidebarVisible}
            aria-controls={sidebarVisible ? "koe-sidebar" : undefined}
            disabled={overBudget}
            onClick={() => setSidebarOpen((v) => !v)}
          >
            <span aria-hidden>☰</span>
          </button>
        </div>

        {showSettings && (
          <div className="koe-main-settings" id="koe-settings-area">
            <SettingsPanel onClose={() => setShowSettings(false)} />
          </div>
        )}

        {/* Conversation area — greeting only until a transcript stream exists
            (koe-sh6); see the honesty note in the header comment. */}
        <section className="koe-conversation" aria-label="会話">
          <h1 className="koe-greeting">{GREETING[status]}</h1>
          {status === "idle" && (
            <p className="koe-greeting-sub">
              下の音声ボタンを押すと、声で話しかけられます
            </p>
          )}
        </section>

        {/* Live activity panel — the hero (透明性＝主役). koe-activity-fill is
            passed in (not reached via a cross-feature descendant selector) so
            ActivityLog owns its own class names (koe-iyr). */}
        <section className="koe-activity-zone" aria-label="ライブ活動">
          <ActivityLog className="koe-activity-fill" />
        </section>

        {/* Voice orb (shrunken from the 2026-06-09 immersive orb) — the primary
            start/stop control, docked under the activity panel. */}
        <footer className="koe-voice-dock">
          <VoiceButton />
        </footer>

        {/* Dev-only event simulator — inside <main> so it never becomes a
            third flex column of the shell. Stripped from production builds
            (import.meta.env.DEV is compile-time false → DCE). */}
        {import.meta.env.DEV && <DevMockEmitter />}
      </main>

      <ApprovalModal />
    </div>
  );
}
