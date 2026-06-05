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
  CostSnapshot,
  SessionStatusEvent,
  ToolEvent,
} from "../../features/activity/types";
import type { AppSettings, PermissionPolicy } from "../../features/settings/types";

/**
 * Allowlisted provider ids — mirror the Rust `provider_key_name` /
 * `tool_provider_key_name` allowlists. Typing the command params as these unions
 * catches a caller typo at compile time instead of only at the backend's
 * runtime rejection.
 */
export type VoiceProvider = "openai" | "google";
export type ToolProvider = "xai" | "x" | "search";
export type Provider = VoiceProvider | ToolProvider;

/** Backend event channels. */
export const EVENT = {
  toolEvent: "tool-event",
  approvalRequired: "tool-approval-required",
  sessionStatus: "session-status",
  // Live monthly cost snapshot pushed on each usage frame (koe-9xi).
  costUpdate: "cost-update",
} as const;

/** Backend command names. */
export const COMMAND = {
  resolveToolApproval: "resolve_tool_approval",
  // Settings commands (koe-200)
  getAppSettings: "get_app_settings",
  completeOnboarding: "complete_onboarding",
  saveBudgetConfig: "save_budget_config",
  setRecorderAdapter: "set_recorder_adapter",
  // Multi-provider settings (koe-31u)
  setVoiceProvider: "set_voice_provider",
  setToolProviderEnabled: "set_tool_provider_enabled",
  deleteToolProviderKey: "delete_tool_provider_key",
  // Permission policy (koe-351)
  setPermissionPolicy: "set_permission_policy",
  pickFolder: "pick_folder",
  // Secret store commands (secret_store.rs)
  setOpenaiApiKey: "set_openai_api_key",
  hasOpenaiApiKey: "has_openai_api_key",
  deleteOpenaiApiKey: "delete_openai_api_key",
  // Multi-provider secret commands (koe-31u) — write + presence only, no get-*
  setProviderApiKey: "set_provider_api_key",
  hasProviderApiKey: "has_provider_api_key",
  deleteProviderApiKey: "delete_provider_api_key",
  // Session lifecycle commands (koe-e3m)
  startSession: "start_session",
  stopSession: "stop_session",
  // Cost snapshot pull (koe-9xi)
  getCostSnapshot: "get_cost_snapshot",
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
 * Subscribe to live monthly-cost snapshots (koe-9xi). The backend pushes one on
 * every usage frame, including the over-budget snapshot just before it stops the
 * session. Returns an unlisten function.
 */
export function onCostUpdate(handler: (snapshot: CostSnapshot) => void): Promise<UnlistenFn> {
  return listen<CostSnapshot>(EVENT.costUpdate, (e) => handler(e.payload));
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
// Multi-provider key + voice/tool settings commands (koe-31u)
//
// Provider ids are resolved by a closed allowlist on the Rust side, so an
// unknown provider is rejected before the vault is touched. As with OpenAI,
// keys are write-only from the WebView — there is deliberately NO
// get_*_api_key command for any provider.
// ---------------------------------------------------------------------------

/** Persists the selected voice provider/model (e.g. "openai/gpt-realtime-2"). */
export function setVoiceProvider(value: string): Promise<void> {
  return invoke(COMMAND.setVoiceProvider, { value });
}

/** Enables/disables a 手足 (tool) provider. Records intent only — not the key.
 *  Enabling is rejected backend-side if no key is stored for the provider. */
export function setToolProviderEnabled(provider: ToolProvider, enabled: boolean): Promise<void> {
  return invoke(COMMAND.setToolProviderEnabled, { provider, enabled });
}

/** Deletes a 手足 tool key AND clears its enable flag atomically (one backend
 *  lock-hold, so a concurrent enable can't leave an "enabled but key-less" state). */
export function deleteToolProviderKey(provider: ToolProvider): Promise<void> {
  return invoke(COMMAND.deleteToolProviderKey, { provider });
}

// ---------------------------------------------------------------------------
// Permission policy commands (koe-351)
// ---------------------------------------------------------------------------

/**
 * Replaces the whole permission policy. The backend validates it (bounds + host
 * well-formedness) before persisting and rejects a malformed policy; the
 * fail-closed evaluation (baseline + deny always win) does the rest at decision
 * time. Preserves all other settings.
 */
export function setPermissionPolicy(policy: PermissionPolicy): Promise<void> {
  return invoke(COMMAND.setPermissionPolicy, { policy });
}

/**
 * Opens the OS native folder picker (Rust-side) and resolves to the chosen
 * absolute path, or `null` if the user cancelled. The dialog runs entirely in
 * Rust — the WebView has no `dialog:*` capability; only the path string crosses
 * the IPC boundary.
 */
export function pickFolder(): Promise<string | null> {
  return invoke<string | null>(COMMAND.pickFolder);
}

/**
 * Stores an API key for an allowlisted provider (voice: `openai` / `google`,
 * 手足: `xai` / `x` / `search`). The key is never returned to the WebView
 * afterwards (no get-* command exists for any provider).
 */
export function setProviderApiKey(provider: Provider, key: string): Promise<void> {
  return invoke(COMMAND.setProviderApiKey, { provider, key });
}

/** Returns whether a key is stored for `provider`, without returning its value. */
export function hasProviderApiKey(provider: Provider): Promise<boolean> {
  return invoke<boolean>(COMMAND.hasProviderApiKey, { provider });
}

/** Deletes the stored key for `provider`. */
export function deleteProviderApiKey(provider: Provider): Promise<void> {
  return invoke(COMMAND.deleteProviderApiKey, { provider });
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

// ---------------------------------------------------------------------------
// Cost snapshot (koe-9xi)
// ---------------------------------------------------------------------------

/**
 * Pulls the current month's cost snapshot (spend so far + cap + over-budget),
 * read from the backend's authoritative ledger. Rejects (fail-closed) if settings
 * or the ledger can't be read — the caller shows an explicit "unknown" state
 * rather than a fabricated $0. Contains no secret values (numbers + a bool only).
 */
export function getCostSnapshot(): Promise<CostSnapshot> {
  return invoke<CostSnapshot>(COMMAND.getCostSnapshot);
}
