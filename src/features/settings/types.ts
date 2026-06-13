// Settings feature — TypeScript contract for the Rust settings_store types.
//
// Rust uses plain `#[derive(Serialize, Deserialize)]` with no serde rename
// attributes, so all field names are serialised as-is (snake_case) over the
// Tauri IPC bridge. The TypeScript types below use the exact keys that the
// backend emits.

/**
 * Budget configuration. Mirrors `cost_tracker::BudgetConfig` (Rust).
 * - `enabled = false` → explicit unlimited (user's deliberate choice).
 * - `enabled = true` → hard cap at `monthly_limit_nanodollars`.
 */
export interface BudgetConfig {
  enabled: boolean;
  /** Limit in nanodollars (1 USD = 1,000,000,000). Arithmetic is done in Rust. */
  monthly_limit_nanodollars: number;
}

/**
 * Per-provider enable flags for the 手足 (tool) keys. Mirrors
 * `settings_store::ToolProviderFlags` (Rust). The key itself lives in the secret
 * store (queried via `hasProviderApiKey`); this only records "user wants it on".
 */
export interface ToolProviderFlags {
  xai: boolean;
  x: boolean;
  search: boolean;
}

/**
 * One allow-listed folder. Mirrors `permission_policy::AllowedFolder` (Rust).
 * `allow_danger` opts this folder into auto-running DANGER ops (delete/…); it
 * defaults to `false`, so DANGER stays gated even inside an allowed folder.
 */
export interface AllowedFolder {
  path: string;
  allow_danger: boolean;
}

/**
 * User permission policy. Mirrors `permission_policy::PermissionPolicy` (Rust).
 * Layered ON TOP of the built-in risk gate: priority is 禁止 > 許可 > 既定, and
 * the backend always wins on the built-in sensitive baseline (.ssh/.env/…) — the
 * allow-lists here only relax the APPROVAL decision, never the real-IO defenses.
 */
export interface PermissionPolicy {
  /** Folders auto-approved (green light); each with a per-folder DANGER opt-in. */
  allowed_folders: AllowedFolder[];
  /** Folders that always require confirmation (禁止 — wins over allow). */
  denied_folders: string[];
  /** URL hosts auto-approved for `open_url` (suffix + dot-boundary match). */
  allowed_url_hosts: string[];
  /** URL hosts that always require confirmation (禁止). */
  denied_url_hosts: string[];
  /** Opt-in: auto-approve `open_url` for ANY http/https host (except a denied one). */
  allow_all_urls: boolean;
}

/** An empty policy (auto-approves nothing) — the safe default + a reset value. */
export const EMPTY_PERMISSION_POLICY: PermissionPolicy = {
  allowed_folders: [],
  denied_folders: [],
  allowed_url_hosts: [],
  denied_url_hosts: [],
  allow_all_urls: false,
};

/**
 * Application settings. Mirrors `settings_store::AppSettings` (Rust).
 * All field names are snake_case to match the JSON the backend emits.
 */
export interface AppSettings {
  onboarding_completed: boolean;
  budget: BudgetConfig;
  /** Recorder adapter name. M1 only supports `"sqlite"`. */
  recorder_adapter: string;
  /**
   * Selected voice provider/model as a single `"provider/model"` string
   * (e.g. `"openai/gpt-realtime-2"`). rhanis-31u persists it; rhanis-zv3 acts on it.
   */
  voice_provider_model: string;
  /** Per-provider enable flags for the 手足 tool keys (rhanis-31u). */
  tool_providers: ToolProviderFlags;
  /** Folder/URL allow + deny permission policy (rhanis-351). */
  permission_policy: PermissionPolicy;
}

/**
 * The UI-side choice the user makes during onboarding.
 * Kept separate from `BudgetConfig` so the form can represent the
 * "not yet chosen" state (neither enabled nor explicitly unlimited).
 */
export type BudgetChoice =
  | { kind: "limited"; monthlyLimitUsd: number }
  | { kind: "unlimited" }
  | { kind: "pending" };
