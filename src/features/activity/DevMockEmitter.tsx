// Dev-only panel that synthesizes backend events, so the operator console can
// be exercised on Windows before the Rust tool_dispatcher (koe-2gy) and
// approval_gate (koe-1vi) exist. Render it only behind `import.meta.env.DEV`.

import { useActivityStore } from "./activityStore";
import type { ApprovalRisk, SessionConnState } from "./types";
import "./DevMockEmitter.css";

let seq = 0;
function nextSeq(): number {
  seq += 1;
  return seq;
}

/** Pushes a start → progress → done sequence for a fake web_search call. */
function emitMockToolRun(): void {
  const actionId = `mock-${nextSeq()}`;
  const base = { actionId, tool: "web_search", timestamp: Date.now() };

  useActivityStore.getState().ingestToolEvent({
    ...base,
    eventId: `${actionId}-start`,
    sequence: nextSeq(),
    phase: "start",
    displaySummary: "「東京の天気」を検索中",
  });

  setTimeout(() => {
    useActivityStore.getState().ingestToolEvent({
      ...base,
      eventId: `${actionId}-progress`,
      sequence: nextSeq(),
      phase: "progress",
      displaySummary: "結果を要約中",
      progress: 0.6,
    });
  }, 1200);

  setTimeout(() => {
    useActivityStore.getState().ingestToolEvent({
      ...base,
      eventId: `${actionId}-done`,
      sequence: nextSeq(),
      phase: "done",
      displaySummary: "検索完了",
    });
  }, 2600);
}

/**
 * Demonstrates the glass-box M1 disclosure (koe-sua.1): a thinking-event is
 * pushed FIRST, then ~400ms later (inside the 300–700ms thinking window) the
 * tool goes live — mirroring the backend's "disclose before you act" ordering so
 * the operator console can be exercised on Windows before a live session exists.
 */
function emitMockThinkingThenTool(): void {
  const actionId = `mock-${nextSeq()}`;

  // 1) The disclosure — emitted before the tool, lower sequence than its start.
  useActivityStore.getState().ingestThinkingEvent({
    eventId: `${actionId}-think`,
    actionId,
    sequence: nextSeq(),
    phase: "deciding",
    plan: "ウェブを検索しています",
    tool: "web_search",
    source: "web",
    timestamp: Date.now(),
  });

  // 2) ~400ms later the tool starts, then completes — the action the disclosure
  //    promised. Same actionId, so a disclosure lines up with its tool call.
  const base = { actionId, tool: "web_search", timestamp: Date.now() };
  setTimeout(() => {
    useActivityStore.getState().ingestToolEvent({
      ...base,
      eventId: `${actionId}-start`,
      sequence: nextSeq(),
      phase: "start",
      displaySummary: "「東京の天気」を検索中",
    });
  }, 400);
  setTimeout(() => {
    useActivityStore.getState().ingestToolEvent({
      ...base,
      eventId: `${actionId}-done`,
      sequence: nextSeq(),
      phase: "done",
      displaySummary: "検索完了",
    });
  }, 1600);
}

/** Enqueues a fake approval with a 30s deadline. */
function emitMockApproval(risk: ApprovalRisk): void {
  const id = `mock-approval-${nextSeq()}`;
  useActivityStore.getState().enqueueApproval({
    approvalId: id,
    tool: risk === "DANGER" ? "run_command" : "open_url",
    risk,
    displaySummary:
      risk === "DANGER"
        ? "`git push --force` を実行しようとしています"
        : "https://example.com を開こうとしています",
    deadlineAt: Date.now() + 30_000,
    sequence: nextSeq(),
  });
}

function setStatus(state: SessionConnState): void {
  useActivityStore.getState().setSessionStatus({
    state,
    error: state === "error" ? "マイクに接続できませんでした" : undefined,
    sequence: nextSeq(),
  });
}

export function DevMockEmitter() {
  return (
    <div className="koe-devmock" aria-label="開発用モック">
      <span className="koe-devmock-label">dev mock</span>
      <button type="button" onClick={emitMockToolRun}>
        tool 実行
      </button>
      <button type="button" onClick={emitMockThinkingThenTool}>
        思考→tool
      </button>
      <button type="button" onClick={() => emitMockApproval("DANGER")}>
        承認要求 (DANGER)
      </button>
      <button type="button" onClick={() => emitMockApproval("CAUTION")}>
        承認要求 (CAUTION)
      </button>
      <button type="button" onClick={() => setStatus("connected")}>
        接続
      </button>
      <button type="button" onClick={() => setStatus("error")}>
        エラー
      </button>
      <button type="button" onClick={() => useActivityStore.getState().reset()}>
        リセット
      </button>
    </div>
  );
}
