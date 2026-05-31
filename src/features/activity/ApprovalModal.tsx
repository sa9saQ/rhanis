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

import { useEffect, useRef, useState } from "react";

import { resolveToolApproval } from "../../lib/tauri/ipc";
import { useActivityStore } from "./activityStore";
import type { ApprovalDecision } from "./types";
import "./ApprovalModal.css";

function secondsLeft(deadlineAt: number, now: number): number {
  return Math.max(0, Math.ceil((deadlineAt - now) / 1000));
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

  return (
    <div className="koe-approval-backdrop" role="dialog" aria-modal="true" aria-label="承認の確認">
      <div className={`koe-approval-card koe-risk-${current.risk.toLowerCase()}`}>
        <div className="koe-approval-top">
          <span className="koe-risk-tag">{current.risk}</span>
          <span className="koe-approval-tool">{current.tool}</span>
          <span className="koe-approval-countdown" aria-label="残り時間">
            残り {remaining}s
          </span>
        </div>

        <p className="koe-approval-summary">{current.displaySummary}</p>

        {ipcError && (
          <p className="koe-approval-error" role="alert">
            {ipcError}
          </p>
        )}

        <div className="koe-approval-actions">
          <button
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
