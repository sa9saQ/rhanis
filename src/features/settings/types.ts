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
   * (e.g. `"openai/gpt-realtime-2"`). koe-31u persists it; koe-zv3 acts on it.
   */
  voice_provider_model: string;
  /** Per-provider enable flags for the 手足 tool keys (koe-31u). */
  tool_providers: ToolProviderFlags;
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
