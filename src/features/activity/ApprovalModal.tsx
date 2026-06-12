// Human-in-the-loop approval for DANGER operations (M1; CAUTION is notify-only
// and never populates this queue — see types.ts ApprovalRisk). Shows the head of
// the FIFO approval queue, a live countdown to the backend's 30s oneshot
// deadline, and approve / deny actions that echo `approvalId` back to Rust.
//
// Safety contract (CLAUDE.md + Codex R-A C1):
//  - The decision is routed by `approvalId`; the backend rejects unknown /
//    timed-out / duplicate ids, so a stale click cannot approve the wrong op.
//  - On deadline we only clear the modal — the backend has already returned
//    "user declined" via its own timeout (fail-closed). We never auto-approve.
//  - The decision is dequeued ONLY on a successful round-trip; if the IPC fails
//    the modal stays open with an error so the user can retry (never a silent
//    close that reads as "approved").

import { type KeyboardEvent as ReactKeyboardEvent, useEffect, useRef, useState } from "react";

import { resolveToolApproval } from "../../lib/tauri/ipc";
import { useActivityStore } from "./activityStore";
import type { ApprovalDecision } from "./types";
import "./ApprovalModal.css";

function secondsLeft(deadlineAt: number, now: number): number {
  return Math.max(0, Math.ceil((deadlineAt - now) / 1000));
}

// The modal's only interactive controls are buttons, so querying enabled buttons
// is the exact (and minimal) focusable set for the focus trap.
function focusableButtons(container: HTMLElement | null): HTMLButtonElement[] {
  if (!container) return [];
  return Array.from(container.querySelectorAll<HTMLButtonElement>("button:not([disabled])"));
}

// Coarse cadence for the SR live region so it does not fire every second: 10s
// buckets above the final 5s, then each of the last 5s (the decision-urgent
// window). The visible counter still ticks every second for sighted users.
function countdownAnnouncement(remaining: number): string {
  if (remaining > 5) return `残り約${Math.ceil(remaining / 10) * 10}秒`;
  return `残り${remaining}秒`;
}

export function ApprovalModal() {
  const current = useActivityStore((s) => s.approvalQueue[0]);
  const waiting = useActivityStore((s) => Math.max(0, s.approvalQueue.length - 1));
  const dequeueApproval = useActivityStore((s) => s.dequeueApproval);

  const [now, setNow] = useState(() => Date.now());
  const [busy, setBusy] = useState(false);
  const [ipcError, setIpcError] = useState<string | null>(null);
  // Synchronous re-entrancy guard (a React state read would be a stale closure
  // for a second click within the same event flush).
  const inFlight = useRef(false);

  // a11y / keyboard (koe-471).
  const dialogRef = useRef<HTMLDivElement | null>(null);
  const denyButtonRef = useRef<HTMLButtonElement | null>(null);
  // The element focused before the modal opened, restored on close.
  const openerRef = useRef<HTMLElement | null>(null);

  const isOpen = current != null;
  const headId = current?.approvalId;

  // Tick once a second while a request is shown; sync `now` immediately when a
  // new head appears so the countdown never flashes a stale (inflated) value.
  useEffect(() => {
    if (!current) return;
    setNow(Date.now());
    setIpcError(null);
    const id = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(id);
  }, [current]);

  // Auto-dismiss once the backend deadline passes (backend already declined).
  useEffect(() => {
    if (!current) return;
    if (now >= current.deadlineAt) {
      dequeueApproval(current.approvalId);
    }
  }, [current, now, dequeueApproval]);

  // Remember the opener on open and restore focus to it on close, so a keyboard
  // user returns to where they were. Keyed on open/close only — it must NOT
  // re-run (and overwrite the opener) when the next queued head appears. Declared
  // before the initial-focus effect so it captures the opener BEFORE focus moves
  // into the modal.
  useEffect(() => {
    if (!isOpen) return;
    openerRef.current = (document.activeElement as HTMLElement | null) ?? null;
    return () => {
      openerRef.current?.focus?.();
      openerRef.current = null;
    };
  }, [isOpen]);

  // Initial focus on the DENY (safe) button for each head, so a stray Enter /
  // Space / Escape can never auto-approve a DANGER op (fail-closed).
  useEffect(() => {
    if (!headId) return;
    denyButtonRef.current?.focus();
  }, [headId]);

  if (!current) return null;

  const remaining = secondsLeft(current.deadlineAt, now);

  async function decide(decision: ApprovalDecision) {
    if (!current || inFlight.current) return;
    const { approvalId } = current;
    inFlight.current = true;
    setBusy(true);
    let failed = false;
    try {
      await resolveToolApproval(approvalId, decision);
    } catch {
      // Do NOT surface the raw backend error: it may carry a path / key / PII.
      // Show a fixed message; the backend's own 30s timeout is the safety net.
      failed = true;
    } finally {
      inFlight.current = false;
      setBusy(false);
      if (failed) {
        // Keep the modal open so the user can retry; do NOT dequeue. Only show
        // the error if this request is still the head (it may have been
        // auto-dismissed on deadline while the IPC was in flight).
        const stillHead = useActivityStore.getState().approvalQueue[0]?.approvalId === approvalId;
        if (stillHead) {
          setIpcError("承認の送信に失敗しました。もう一度お試しください。");
        }
      } else {
        dequeueApproval(approvalId);
      }
    }
  }

  // Trap Tab within the dialog and map Escape to deny (fail-closed). Enter /
  // Space activate the focused button natively, so they need no handling here.
  function onDialogKeyDown(e: ReactKeyboardEvent<HTMLDivElement>) {
    if (e.key === "Escape") {
      e.preventDefault();
      void decide("deny");
      return;
    }
    if (e.key !== "Tab") return;
    const focusables = focusableButtons(dialogRef.current);
    if (focusables.length === 0) {
      e.preventDefault(); // nothing focusable (in-flight) — keep focus trapped
      return;
    }
    const first = focusables[0];
    const last = focusables[focusables.length - 1];
    const active = document.activeElement;
    const within = dialogRef.current?.contains(active) ?? false;
    if (e.shiftKey) {
      if (!within || active === first) {
        e.preventDefault();
        last.focus();
      }
    } else if (!within || active === last) {
      e.preventDefault();
      first.focus();
    }
  }

  return (
    <div
      ref={dialogRef}
      className="koe-approval-backdrop"
      role="dialog"
      aria-modal="true"
      aria-label="承認の確認"
      onKeyDown={onDialogKeyDown}
    >
      <div className={`koe-approval-card koe-risk-${current.risk.toLowerCase()}`}>
        <div className="koe-approval-top">
          <span className="koe-risk-tag">{current.risk}</span>
          <span className="koe-approval-tool">{current.tool}</span>
          {/* Visible counter ticks every second; hidden from SR to avoid a
              per-second barrage — the live region below reads a coarser cadence. */}
          <span className="koe-approval-countdown" aria-hidden="true">
            残り {remaining}s
          </span>
          <span className="koe-visually-hidden" aria-live="polite">
            {countdownAnnouncement(remaining)}
          </span>
        </div>

        {/* displaySummary carries a model-influenced target descriptor
            (koe-whf) — monospace marks it as DATA, not UI chrome, so an
            instruction-like filename ("safe-click-approve.txt") reads as a
            filename, never as the app speaking. */}
        <p className="koe-approval-summary">
          <code>{current.displaySummary}</code>
        </p>

        {ipcError && (
          <p className="koe-approval-error" role="alert">
            {ipcError}
          </p>
        )}

        <div className="koe-approval-actions">
          <button
            ref={denyButtonRef}
            type="button"
            className="koe-btn koe-btn-deny"
            disabled={busy}
            onClick={() => void decide("deny")}
          >
            拒否
          </button>
          <button
            type="button"
            className="koe-btn koe-btn-approve"
            disabled={busy}
            onClick={() => void decide("approve")}
          >
            許可
          </button>
        </div>

        {waiting > 0 && <p className="koe-approval-more">他に {waiting} 件の承認待ち</p>}
      </div>
    </div>
  );
}
