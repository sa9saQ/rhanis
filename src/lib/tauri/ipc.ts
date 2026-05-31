// Type-safe wrappers over Tauri's `invoke` / `listen`.
//
// Centralising the event-name and command-name strings here keeps the
// backend contract in one place and lets components subscribe without
// repeating channel names or casting `unknown` payloads.

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import type {
  ApprovalDecision,
  ApprovalRequest,
  SessionStatusEvent,
  ToolEvent,
} from "../../features/activity/types";
import type { AppSettings } from "../../features/settings/types";

/** Backend event channels. */
export const EVENT = {
  toolEvent: "tool-event",
  approvalRequired: "tool-approval-required",
  sessionStatus: "session-status",
} as const;

/** Backend command names. */
export const COMMAND = {
  resolveToolApproval: "resolve_tool_approval",
  // Settings commands (koe-200)
  getAppSettings: "get_app_settings",
  completeOnboarding: "complete_onboarding",
  saveBudgetConfig: "save_budget_config",
  setRecorderAdapter: "set_recorder_adapter",
  // Secret store commands (secret_store.rs)
  setOpenaiApiKey: "set_openai_api_key",
  hasOpenaiApiKey: "has_openai_api_key",
  deleteOpenaiApiKey: "delete_openai_api_key",
  // Session lifecycle commands (koe-e3m)
  startSession: "start_session",
  stopSession: "stop_session",
} as const;

/** Subscribe to live tool events. Returns an unlisten function. */
export function onToolEvent(handler: (event: ToolEvent) => void): Promise<UnlistenFn> {
  return listen<ToolEvent>(EVENT.toolEvent, (e) => handler(e.payload));
}

/** Subscribe to approval requests. Returns an unlisten function. */
export function onApprovalRequired(
  handler: (request: ApprovalRequest) => void,
): Promise<UnlistenFn> {
  return listen<ApprovalRequest>(EVENT.approvalRequired, (e) => handler(e.payload));
}

/** Subscribe to session connection-status changes. Returns an unlisten function. */
export function onSessionStatus(
  handler: (status: SessionStatusEvent) => void,
): Promise<UnlistenFn> {
  return listen<SessionStatusEvent>(EVENT.sessionStatus, (e) => handler(e.payload));
}

/**
 * Resolve a pending approval. The backend routes the decision to the matching
 * oneshot by `approvalId`; unknown / timed-out / duplicate ids are rejected
 * backend-side (fail-closed).
 */
export function resolveToolApproval(
  approvalId: string,
  decision: ApprovalDecision,
): Promise<void> {
  return invoke(COMMAND.resolveToolApproval, { approvalId, decision });
}

// ---------------------------------------------------------------------------
// Settings commands (koe-200)
// ---------------------------------------------------------------------------

/** Returns the persisted app settings. Contains no secret values. */
export function getAppSettings(): Promise<AppSettings> {
  return invoke<AppSettings>(COMMAND.getAppSettings);
}

/**
 * Completes first-run onboarding. Persists the budget choice and recorder
 * adapter atomically.
 *
 * Tauri automatically converts camelCase JS arg keys → snake_case Rust params.
 */
export function completeOnboarding(
  enabled: boolean,
  monthlyLimitUsd: number | null,
  recorderAdapter: string,
): Promise<void> {
  return invoke(COMMAND.completeOnboarding, { enabled, monthlyLimitUsd, recorderAdapter });
}

/**
 * Updates the budget configuration after onboarding. Preserves other settings.
 */
export function saveBudgetConfig(
  enabled: boolean,
  monthlyLimitUsd: number | null,
): Promise<void> {
  return invoke(COMMAND.saveBudgetConfig, { enabled, monthlyLimitUsd });
}

/** Updates the recorder adapter. M1 only accepts `"sqlite"`. */
export function setRecorderAdapter(name: string): Promise<void> {
  return invoke(COMMAND.setRecorderAdapter, { name });
}

// ---------------------------------------------------------------------------
// Secret store commands (secret_store.rs)
// ---------------------------------------------------------------------------

/**
 * Stores the OpenAI API key in the encrypted vault. The key is never returned
 * to the WebView after this call (no `get_openai_api_key` command exists).
 */
export function setOpenaiApiKey(key: string): Promise<void> {
  return invoke(COMMAND.setOpenaiApiKey, { key });
}

/** Returns whether an OpenAI API key is currently stored. */
export function hasOpenaiApiKey(): Promise<boolean> {
  return invoke<boolean>(COMMAND.hasOpenaiApiKey);
}

/** Deletes the stored OpenAI API key. */
export function deleteOpenaiApiKey(): Promise<void> {
  return invoke(COMMAND.deleteOpenaiApiKey);
}

// ---------------------------------------------------------------------------
// Session lifecycle commands (koe-e3m)
// ---------------------------------------------------------------------------

/**
 * Starts a Realtime session: connects the BYOK WebSocket, registers tools, and
 * begins the read loop. Rejects (fail-closed) if onboarding is incomplete, the
 * monthly budget is exceeded, or no API key is stored. Live connection status
 * arrives on the `session-status` channel.
 */
export function startSession(): Promise<void> {
  return invoke(COMMAND.startSession);
}

/** Stops the active Realtime session. Idempotent (no-op if already stopped). */
export function stopSession(): Promise<void> {
  return invoke(COMMAND.stopSession);
}
