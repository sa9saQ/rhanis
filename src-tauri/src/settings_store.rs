//! App settings persistence — Rust-owned JSON at `app_local_data_dir/koe-settings.json`.
//!
//! # Design
//! - Settings are stored as JSON in the per-user app data dir. No WebView SQL
//!   or plugin surface exists (same posture as `secret_store.rs` / `adapter.rs`).
//! - [`SettingsError`]'s `Display` returns **fixed** strings; no path, value, or
//!   OS detail can leak to the WebView (mirrors `RecorderError` / `SecretError`).
//! - `load` when the file is absent → [`AppSettings::default()`] (first run).
//! - `load` when the file is present but corrupt → **Err (fail-closed)**.
//!   Silently resetting to default would erase a user's budget cap.
//! - `save` is **atomic**: write to `<path>.tmp`, then `fs::rename` over the
//!   target (rename is atomic on the same filesystem; partial writes never replace
//!   the live file).
//!
//! transaction N/A · idempotency_key N/A (local settings file, not billing)

use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::cost_tracker::{usd_to_nanodollars, BudgetConfig};
use crate::permission_policy::{
    validate_permission_policy, PermissionPolicy, PolicyProvider, PolicyState,
};
use crate::secret_store::{
    ManagedSecretStore, OPENAI_KEY_NAME, SEARCH_KEY_NAME, XAI_KEY_NAME, X_API_KEY_NAME,
};

// ---------------------------------------------------------------------------
// AppSettings
// ---------------------------------------------------------------------------

/// Persisted application settings. Serialised as JSON to
/// `app_local_data_dir/koe-settings.json` via [`JsonSettingsStore`].
///
/// Non-safety fields carry serde defaults (so a future-added field does not fail
/// an older file), but [`budget`](AppSettings::budget) is **required** — it is a
/// safety control, and silently defaulting a missing `budget` to "unlimited"
/// would erase a user's cap. A file missing `budget` therefore fails to
/// deserialise → [`SettingsError::Corrupt`] (fail-closed), not a silent reset.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppSettings {
    /// Whether the user has completed first-run onboarding (budget choice +
    /// API key entry). The UI gate blocks the activity console until this is
    /// `true`. Backend enforcement is `session_manager`'s responsibility
    /// (koe-e3m — deliberate seam, not skeleton). Defaults to `false`
    /// (fail-closed: a missing flag means "not onboarded").
    #[serde(default)]
    pub onboarding_completed: bool,

    /// Budget configuration. `enabled = false` means the user explicitly chose
    /// unlimited; `true` with a non-zero limit means a hard cap is active.
    /// **Required** (no serde default) — see the type doc above.
    pub budget: BudgetConfig,

    /// Which recorder backend to use. M1 only supports `"sqlite"`.
    #[serde(default = "default_recorder_adapter")]
    pub recorder_adapter: String,

    /// Selected voice provider/model as a single `"provider/model"` string
    /// (e.g. `"openai/gpt-realtime-2"`). koe-31u only PERSISTS this; the actual
    /// connection switch is koe-zv3 (which parses it). Non-safety metadata, so it
    /// carries a serde default — an older settings file migrates silently.
    #[serde(default = "default_voice_provider_model")]
    pub voice_provider_model: String,

    /// Which 手足 (tool) providers the user has enabled. Stores ONLY the enable
    /// flag — the key itself lives in the secret store (queried live via
    /// `has_provider_api_key`), never persisted here. A typed struct (not a map)
    /// so a hand-edited file cannot inject arbitrary keys.
    #[serde(default)]
    pub tool_providers: ToolProviderFlags,

    /// User permission policy (koe-351): folder/URL allow + deny lists layered on
    /// top of the built-in risk gate. Absent → [`PermissionPolicy::default()`] (an
    /// empty policy that auto-approves NOTHING), so an older file migrates safely
    /// and a missing policy is fail-closed by construction — unlike `budget`, the
    /// safe default here IS the default value, so `#[serde(default)]` is correct.
    #[serde(default)]
    pub permission_policy: PermissionPolicy,
}

/// Per-provider enable flags for the 手足 (tool) keys (koe-31u). Keys live in the
/// secret store; this only records "the user wants this tool active". koe-eal
/// consumes these to decide which tools to register. Non-safety metadata, fully
/// defaulted so an older file (without this object) loads as all-disabled.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ToolProviderFlags {
    #[serde(default)]
    pub xai: bool,
    #[serde(default)]
    pub x: bool,
    #[serde(default)]
    pub search: bool,
}

fn default_recorder_adapter() -> String {
    "sqlite".into()
}

fn default_voice_provider_model() -> String {
    "openai/gpt-realtime-2".into()
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            onboarding_completed: false,
            budget: BudgetConfig::default(),
            recorder_adapter: default_recorder_adapter(),
            voice_provider_model: default_voice_provider_model(),
            tool_providers: ToolProviderFlags::default(),
            permission_policy: PermissionPolicy::default(),
        }
    }
}

// ---------------------------------------------------------------------------
// Errors — fixed messages only, never echo the underlying cause.
// ---------------------------------------------------------------------------

/// Error returned by the settings store. `Display` returns a **fixed** message
/// per variant so no path, JSON detail, or OS error leaks to the WebView.
#[derive(Debug, PartialEq, Eq)]
pub enum SettingsError {
    /// The data directory is unavailable (permissions, out-of-space, …).
    Unavailable,
    /// The settings file exists but its contents cannot be deserialised (corrupt
    /// or incompatible format). **Fail-closed** — callers must not silently
    /// fall back to defaults, as that would erase a budget cap.
    Corrupt,
}

impl fmt::Display for SettingsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let msg = match self {
            SettingsError::Unavailable => "settings storage is unavailable",
            SettingsError::Corrupt => "settings file is corrupt",
        };
        f.write_str(msg)
    }
}

impl std::error::Error for SettingsError {}

// ---------------------------------------------------------------------------
// SettingsStore trait
// ---------------------------------------------------------------------------

/// Abstraction over the settings backend. M1 uses [`JsonSettingsStore`].
pub trait SettingsStore: Send + Sync {
    fn load(&self) -> Result<AppSettings, SettingsError>;
    fn save(&self, settings: &AppSettings) -> Result<(), SettingsError>;
}

// ---------------------------------------------------------------------------
// JsonSettingsStore — the real M1 implementation.
// ---------------------------------------------------------------------------

/// Persists settings as a JSON file at `path`. Saves write a `.tmp` sibling then
/// `rename` over the target — an atomic swap on the same filesystem, so a
/// **process** crash mid-save never leaves a partially-written live file.
///
/// NOTE: this is not full power-loss durability — there is no `fsync` of the
/// temp file or the parent directory, so a power cut could still lose the most
/// recent write. Acceptable for M1 settings (low-write, user-recoverable);
/// fsync + a save mutex are a tracked follow-up.
pub struct JsonSettingsStore {
    pub path: PathBuf,
}

impl JsonSettingsStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl SettingsStore for JsonSettingsStore {
    fn load(&self) -> Result<AppSettings, SettingsError> {
        if !self.path.exists() {
            return Ok(AppSettings::default());
        }
        let bytes = std::fs::read(&self.path).map_err(|_| SettingsError::Unavailable)?;
        let settings: AppSettings =
            serde_json::from_slice(&bytes).map_err(|_| SettingsError::Corrupt)?;
        // A valid-JSON-but-out-of-range file (hand-edited / tampered, e.g. a huge
        // monthly_limit_nanodollars or enabled=false with a non-zero limit) must
        // fail closed on the READ path too — the save-path bound alone would let
        // such a file load with an effectively-unlimited cap and pass the gate.
        validate_app_settings(&settings)?;
        Ok(settings)
    }

    fn save(&self, settings: &AppSettings) -> Result<(), SettingsError> {
        // Create parent directory if needed (first run before the data dir exists).
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|_| SettingsError::Unavailable)?;
        }

        let tmp_path = self.path.with_extension("json.tmp");
        let json = serde_json::to_vec_pretty(settings).map_err(|_| SettingsError::Unavailable)?;

        // Write to the temp file first.
        std::fs::write(&tmp_path, &json).map_err(|_| SettingsError::Unavailable)?;

        // Atomic rename: on the same filesystem this is crash-safe.
        std::fs::rename(&tmp_path, &self.path).map_err(|_| SettingsError::Unavailable)?;

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Managed state + Tauri commands (WebView surface).
// ---------------------------------------------------------------------------

/// Tauri managed-state wrapper around the active [`SettingsStore`].
///
/// Field `.0` is the store (read by `get_app_settings` / `session_manager`).
/// Field `.1` is a write lock that serialises the **compound load-modify-save**
/// command sequences via [`ManagedSettings::update`], so concurrent settings
/// writers (e.g. rapid 手足 tool-toggle clicks, each its own async IPC) cannot
/// lose each other's updates (last-writer-wins). Construct with
/// [`ManagedSettings::new`]. Reads need no lock — saves are atomic temp+rename,
/// so a concurrent read sees the whole old or whole new file, never a torn one.
pub struct ManagedSettings(pub Arc<dyn SettingsStore>, std::sync::Mutex<()>);

impl ManagedSettings {
    pub fn new(store: Arc<dyn SettingsStore>) -> Self {
        Self(store, std::sync::Mutex::new(()))
    }

    /// Runs `load → mutate → save` under the write lock so two concurrent
    /// mutating commands can't read the same base and clobber each other. The
    /// lock is held only across the synchronous load+save (no `.await` inside),
    /// so it never blocks the async runtime. If `f` returns `Err`, nothing is
    /// saved (the in-memory mutation is discarded — no partial write).
    /// Runs `f` while holding the settings write lock, handing it the store so a
    /// command can keep settings **and** an external store (the secret vault)
    /// consistent across more than one operation without another writer racing
    /// in between. The lock is held only across synchronous work (no `.await`
    /// inside), so it never blocks the async runtime. NOTE: `f` must use the
    /// `store` arg and must NOT re-enter `update`/`replace` (the lock is not
    /// reentrant — that would deadlock).
    fn with_write_lock<F, T>(&self, f: F) -> Result<T, String>
    where
        F: FnOnce(&dyn SettingsStore) -> Result<T, String>,
    {
        // PoisonError → fixed message (a poisoned lock means a prior writer
        // panicked; surface it as "unavailable", never as a silent success).
        let _guard = self
            .1
            .lock()
            .map_err(|_| SettingsError::Unavailable.to_string())?;
        f(self.0.as_ref())
    }

    /// `load → mutate → save` under the write lock so two concurrent mutating
    /// commands can't read the same base and clobber each other. If `f` returns
    /// `Err`, nothing is saved (the in-memory mutation is discarded).
    fn update<F>(&self, f: F) -> Result<(), String>
    where
        F: FnOnce(&mut AppSettings) -> Result<(), String>,
    {
        self.with_write_lock(|store| {
            let mut current = store.load().map_err(|e| e.to_string())?;
            f(&mut current)?;
            store.save(&current).map_err(|e| e.to_string())
        })
    }

    /// Saves a fully-constructed settings object under the write lock. Used by
    /// `complete_onboarding`, which builds the object from scratch (not a
    /// load-modify), so that path is serialised with the other writers too — no
    /// settings write bypasses the lock.
    fn replace(&self, settings: &AppSettings) -> Result<(), String> {
        self.with_write_lock(|store| store.save(settings).map_err(|e| e.to_string()))
    }
}

/// Bridges the settings store to the tool dispatcher's [`PolicyProvider`] seam
/// (koe-351). Reads the policy from the store on EACH dispatch so a UI edit takes
/// effect immediately. A load failure (corrupt/unreadable) maps to
/// [`PolicyState::Unavailable`] — NOT an empty policy — so a transient failure
/// cannot silently drop the user's deny list (the dispatcher then forces a human
/// decision for any policy-relevant target). Holds the same `Arc<dyn SettingsStore>`
/// that `ManagedSettings` wraps; reads need no write lock (saves are atomic
/// temp+rename, so a read sees a whole old or whole new file, never a torn one).
pub struct SettingsPolicyProvider(pub Arc<dyn SettingsStore>);

impl PolicyProvider for SettingsPolicyProvider {
    fn current_policy(&self) -> PolicyState {
        match self.0.load() {
            Ok(settings) => PolicyState::Loaded(settings.permission_policy),
            Err(_) => PolicyState::Unavailable,
        }
    }
}

/// Returns the current app settings. Contains **no** secret values; safe for
/// the WebView.
#[tauri::command]
pub async fn get_app_settings(
    settings: tauri::State<'_, ManagedSettings>,
) -> Result<AppSettings, String> {
    settings.0.load().map_err(|e| e.to_string())
}

/// Called once during first-run onboarding. Persists the budget choice (a hard
/// cap **or** explicit unlimited) together with the chosen recorder adapter, and
/// flips `onboarding_completed` to `true`.
///
/// Validation:
/// - A BYOK key must already be stored (`has_api_key`) — onboarding is only
///   "complete" with a key, so neither the UI flow nor a direct IPC call can
///   leave the console reachable keyless. (Deleting the key *after* onboarding
///   is handled by the session-start gate in koe-e3m.)
/// - `recorder_adapter` must be `"sqlite"` (M1 only).
/// - If `enabled`, `monthly_limit_usd` must be `Some`, `> 0`, and `<= 1_000_000`.
/// - If `!enabled` (explicit unlimited), the limit is stored as `0`.
#[tauri::command]
pub async fn complete_onboarding(
    enabled: bool,
    monthly_limit_usd: Option<f64>,
    recorder_adapter: String,
    settings: tauri::State<'_, ManagedSettings>,
    secret: tauri::State<'_, ManagedSecretStore>,
) -> Result<(), String> {
    validate_recorder_adapter(&recorder_adapter)?;

    // Fail-closed: an Err (locked / corrupt vault) is treated as "no key",
    // never as "key present".
    if !secret
        .0
        .has_api_key(OPENAI_KEY_NAME)
        .map_err(|e| e.to_string())?
    {
        return Err("an API key must be stored before completing onboarding".to_string());
    }

    let budget = build_budget_config(enabled, monthly_limit_usd)?;

    let new_settings = AppSettings {
        onboarding_completed: true,
        budget,
        recorder_adapter,
        // Voice/tool selections are made post-onboarding in Settings, so first-run
        // takes the defaults (OpenAI voice, all tools disabled, empty permission
        // policy = auto-approve nothing).
        voice_provider_model: default_voice_provider_model(),
        tool_providers: ToolProviderFlags::default(),
        permission_policy: PermissionPolicy::default(),
    };

    settings.replace(&new_settings)
}

/// Updates the budget configuration after onboarding. Preserves the existing
/// `onboarding_completed` and `recorder_adapter` values.
#[tauri::command]
pub async fn save_budget_config(
    enabled: bool,
    monthly_limit_usd: Option<f64>,
    settings: tauri::State<'_, ManagedSettings>,
) -> Result<(), String> {
    let budget = build_budget_config(enabled, monthly_limit_usd)?;
    settings.update(|s| {
        s.budget = budget;
        Ok(())
    })
}

/// Updates the recorder adapter. M1 only accepts `"sqlite"`.
#[tauri::command]
pub async fn set_recorder_adapter(
    name: String,
    settings: tauri::State<'_, ManagedSettings>,
) -> Result<(), String> {
    validate_recorder_adapter(&name)?;
    settings.update(|s| {
        s.recorder_adapter = name;
        Ok(())
    })
}

/// Sets the selected voice provider/model (koe-31u). PERSISTS only — the actual
/// connection switch is koe-zv3. Validated against the known set so a direct IPC
/// call cannot store an unsupported value (fail-closed). Preserves other fields.
#[tauri::command]
pub async fn set_voice_provider(
    value: String,
    settings: tauri::State<'_, ManagedSettings>,
) -> Result<(), String> {
    validate_voice_provider_model(&value)?;
    settings.update(|s| {
        s.voice_provider_model = value;
        Ok(())
    })
}

/// Enables/disables a 手足 (tool) provider (koe-31u). Records intent only — the
/// key itself is managed via the secret-store commands. Unknown providers are
/// rejected (fixed error, fail-closed). Flips only the targeted flag.
#[tauri::command]
pub async fn set_tool_provider_enabled(
    provider: String,
    enabled: bool,
    settings: tauri::State<'_, ManagedSettings>,
    secret: tauri::State<'_, ManagedSecretStore>,
) -> Result<(), String> {
    // Resolve + validate the provider first (fixed Err, never echoes the id).
    let key_name = tool_provider_key_name(&provider)?;
    // Backend invariant: a tool can only be enabled while its key is actually
    // stored. Don't trust the UI's disabled checkbox — a direct or stale WebView
    // call must not persist a credential-less "enabled" provider for the future
    // tool path (koe-eal) to trust. The key-presence check AND the flag write
    // run under ONE settings-lock hold, so a concurrent delete_tool_provider_key
    // can't slip between them. Fail-closed: an Err from has_api_key (locked
    // vault) blocks the enable too.
    settings.with_write_lock(|store| {
        if enabled && !secret.0.has_api_key(key_name).map_err(|e| e.to_string())? {
            return Err("set an API key before enabling this tool".into());
        }
        let mut s = store.load().map_err(|e| e.to_string())?;
        set_tool_flag(&mut s.tool_providers, &provider, enabled);
        store.save(&s).map_err(|e| e.to_string())
    })
}

/// Deletes a 手足 tool key **and** clears its enable flag, both under ONE
/// settings-lock hold so a concurrent `set_tool_provider_enabled(true)` can't
/// re-enable the provider in between (which would leave the unsafe enabled=true +
/// key-absent state). Order within the lock: clear the flag, then delete the key
/// — a partial failure (the delete errors) therefore leaves disabled + key-
/// present (benign — the tool is simply off). The frontend routes tool-key
/// deletes through here rather than the generic delete + a best-effort flag clear.
#[tauri::command]
pub async fn delete_tool_provider_key(
    provider: String,
    settings: tauri::State<'_, ManagedSettings>,
    secret: tauri::State<'_, ManagedSecretStore>,
) -> Result<(), String> {
    let key_name = tool_provider_key_name(&provider)?;
    settings.with_write_lock(|store| {
        let mut s = store.load().map_err(|e| e.to_string())?;
        set_tool_flag(&mut s.tool_providers, &provider, false);
        store.save(&s).map_err(|e| e.to_string())?;
        secret.0.delete_api_key(key_name).map_err(|e| e.to_string())
    })
}

/// Replaces the whole permission policy (koe-351). The policy is validated
/// (bounds + host well-formedness) BEFORE persisting, so a malformed policy from
/// a direct/stale IPC call is rejected with a fixed message and never written;
/// the existing fail-closed evaluation (baseline + deny always win) does the rest
/// at decision time. Preserves all other settings via the write lock.
#[tauri::command]
pub async fn set_permission_policy(
    policy: PermissionPolicy,
    settings: tauri::State<'_, ManagedSettings>,
) -> Result<(), String> {
    validate_permission_policy(&policy).map_err(|e| e.to_string())?;
    settings.update(|s| {
        s.permission_policy = policy;
        Ok(())
    })
}

/// Maps a 手足 tool provider id to its secret record name (tools only — voice
/// providers are not togglable here). Unknown ids get a fixed Err.
fn tool_provider_key_name(provider: &str) -> Result<&'static str, String> {
    match provider {
        "xai" => Ok(XAI_KEY_NAME),
        "x" => Ok(X_API_KEY_NAME),
        "search" => Ok(SEARCH_KEY_NAME),
        _ => Err("unsupported tool provider".into()),
    }
}

/// Sets the flag for an already-validated tool provider. `provider` MUST have
/// passed [`tool_provider_key_name`]; an unrecognised id is unreachable.
fn set_tool_flag(flags: &mut ToolProviderFlags, provider: &str, enabled: bool) {
    match provider {
        "xai" => flags.xai = enabled,
        "x" => flags.x = enabled,
        "search" => flags.search = enabled,
        _ => unreachable!("provider validated by tool_provider_key_name"),
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn validate_recorder_adapter(name: &str) -> Result<(), String> {
    if name == "sqlite" {
        Ok(())
    } else {
        Err("unsupported recorder adapter".into())
    }
}

/// The voice provider/model strings koe recognises. koe-31u only PERSISTS the
/// choice (koe-zv3 acts on it). Both are listed so a value the UI offers now
/// (OpenAI) or a value a later koe-zv3 build writes (Google) validates; the M1 UI
/// presents Google as a disabled preview. The exact Google model id is confirmed
/// when koe-zv3 wires the Gemini Live connection.
const KNOWN_VOICE_PROVIDER_MODELS: &[&str] =
    &["openai/gpt-realtime-2", "google/gemini-2.5-flash-live"];

fn is_known_voice_provider_model(value: &str) -> bool {
    KNOWN_VOICE_PROVIDER_MODELS.contains(&value)
}

fn validate_voice_provider_model(value: &str) -> Result<(), String> {
    if is_known_voice_provider_model(value) {
        Ok(())
    } else {
        Err("unsupported voice provider".into())
    }
}

/// Authoritative upper bound on an enabled monthly cap (USD). The UI also caps
/// input, but a direct IPC call must not be able to persist a near-unlimited
/// "limited" budget (e.g. 1e10 USD), so the Rust side is the source of truth.
const MAX_MONTHLY_LIMIT_USD: f64 = 1_000_000.0;

/// The same ceiling expressed in nanodollars, for validating a *loaded* budget
/// (which is already stored as integer nanodollars). 1_000_000 USD * 1e9 = 1e15,
/// well within u64.
const MAX_MONTHLY_LIMIT_NANODOLLARS: u64 = 1_000_000 * crate::cost_tracker::NANODOLLARS_PER_USD;

/// Validates a deserialized [`AppSettings`] against the SAME invariants the write
/// path enforces, so a hand-edited / tampered file (valid JSON, out-of-range
/// values) fails closed on load rather than silently disabling or inflating the
/// budget safety control:
/// - `recorder_adapter` must be the only M1-supported backend (`"sqlite"`).
/// - an **enabled** budget must have `0 < limit <= MAX_MONTHLY_LIMIT_NANODOLLARS`.
/// - a **disabled** (explicit-unlimited) budget must have a zero limit.
fn validate_app_settings(s: &AppSettings) -> Result<(), SettingsError> {
    if s.recorder_adapter != "sqlite" {
        return Err(SettingsError::Corrupt);
    }
    // Voice selection must be a known provider/model. An absent field migrates to
    // the default ("openai/gpt-realtime-2", in the known set), so an older file
    // still loads; an unknown value (tampered) fails closed. tool_providers needs
    // no validation — every bool combination is valid and an absent object
    // defaults to all-disabled.
    if !is_known_voice_provider_model(&s.voice_provider_model) {
        return Err(SettingsError::Corrupt);
    }
    if s.budget.enabled {
        let n = s.budget.monthly_limit_nanodollars;
        if !(n > 0 && n <= MAX_MONTHLY_LIMIT_NANODOLLARS) {
            return Err(SettingsError::Corrupt);
        }
    } else if s.budget.monthly_limit_nanodollars != 0 {
        return Err(SettingsError::Corrupt);
    }
    // Permission policy bounds + host well-formedness (koe-351). A tampered file
    // with an over-cap list or a malformed host entry fails closed on load, the
    // same posture as the budget bounds above. The UI keeps legit input valid.
    if validate_permission_policy(&s.permission_policy).is_err() {
        return Err(SettingsError::Corrupt);
    }
    Ok(())
}

fn build_budget_config(enabled: bool, monthly_limit_usd: Option<f64>) -> Result<BudgetConfig, String> {
    let monthly_limit_nanodollars = if enabled {
        let usd = monthly_limit_usd.ok_or("invalid budget amount")?;
        // One check rejects NaN (any comparison with NaN is false), <= 0, Inf,
        // and anything above the authoritative ceiling.
        if !(usd > 0.0 && usd <= MAX_MONTHLY_LIMIT_USD) {
            return Err("invalid budget amount".to_string());
        }
        usd_to_nanodollars(usd).ok_or("invalid budget amount")?
    } else {
        0
    };
    Ok(BudgetConfig {
        enabled,
        monthly_limit_nanodollars,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_store() -> (JsonSettingsStore, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("koe-settings.json");
        (JsonSettingsStore::new(path), dir)
    }

    // ---- SettingsError fixed messages -------------------------------------

    #[test]
    fn settings_error_messages_are_fixed_and_leak_free() {
        for e in [SettingsError::Unavailable, SettingsError::Corrupt] {
            let msg = e.to_string();
            // No path separators
            assert!(!msg.contains('/'), "message contains '/': {msg}");
            assert!(!msg.contains('\\'), "message contains '\\': {msg}");
            // No digits that could carry specifics
            assert!(
                !msg.chars().any(|c| c.is_ascii_digit()),
                "message contains a digit: {msg}"
            );
        }
        assert_eq!(
            SettingsError::Unavailable.to_string(),
            "settings storage is unavailable"
        );
        assert_eq!(SettingsError::Corrupt.to_string(), "settings file is corrupt");
    }

    #[test]
    fn settings_error_is_std_error() {
        let _boxed: Box<dyn std::error::Error> = Box::new(SettingsError::Unavailable);
        let _as_ref: &dyn std::error::Error = &SettingsError::Corrupt;
    }

    // ---- Default ----------------------------------------------------------

    #[test]
    fn default_settings_are_sane() {
        let s = AppSettings::default();
        assert!(!s.onboarding_completed);
        assert!(!s.budget.enabled);
        assert_eq!(s.budget.monthly_limit_nanodollars, 0);
        assert_eq!(s.recorder_adapter, "sqlite");
        assert_eq!(s.voice_provider_model, "openai/gpt-realtime-2");
        assert_eq!(s.tool_providers, ToolProviderFlags::default());
        assert!(!s.tool_providers.xai && !s.tool_providers.x && !s.tool_providers.search);
    }

    // ---- Load absent → default --------------------------------------------

    #[test]
    fn load_absent_returns_default() {
        let (store, _dir) = temp_store();
        let settings = store.load().expect("load absent");
        assert_eq!(settings, AppSettings::default());
    }

    // ---- Save → load round-trip -------------------------------------------

    #[test]
    fn save_load_round_trips() {
        let (store, _dir) = temp_store();
        let original = AppSettings {
            onboarding_completed: true,
            budget: BudgetConfig {
                enabled: true,
                monthly_limit_nanodollars: 50_000_000_000,
            },
            recorder_adapter: "sqlite".into(),
            voice_provider_model: "openai/gpt-realtime-2".into(),
            tool_providers: ToolProviderFlags { xai: true, x: false, search: true },
            permission_policy: crate::permission_policy::PermissionPolicy {
                allowed_folders: vec![crate::permission_policy::AllowedFolder {
                    path: "/home/u/work".into(),
                    allow_danger: true,
                }],
                denied_folders: vec!["/home/u/secret".into()],
                allowed_url_hosts: vec!["openai.com".into()],
                denied_url_hosts: vec!["evil.com".into()],
                allow_all_urls: false,
            },
        };
        store.save(&original).expect("save");
        let loaded = store.load().expect("load");
        assert_eq!(loaded, original);
    }

    // ---- Corrupt JSON → Err (fail-closed) ---------------------------------

    #[test]
    fn corrupt_json_returns_err_not_default() {
        let (store, _dir) = temp_store();
        // Write syntactically invalid JSON.
        std::fs::write(&store.path, b"not json at all {{{ broken").expect("seed corrupt file");
        match store.load() {
            Err(SettingsError::Corrupt) => {} // correct
            other => panic!("expected Err(Corrupt), got {other:?}"),
        }
    }

    #[test]
    fn wrong_type_json_returns_corrupt_not_default() {
        // Syntactically valid JSON but with wrong types for the fields.
        // serde_json rejects type mismatches → must return Err(Corrupt),
        // never silently fall back to Default (which would erase a budget cap).
        let (store, _dir) = temp_store();
        std::fs::write(
            &store.path,
            br#"{"onboarding_completed": "yes", "budget": 5, "recorder_adapter": true}"#,
        )
        .expect("seed wrong-type file");
        match store.load() {
            Err(SettingsError::Corrupt) => {} // correct
            other => panic!("expected Err(Corrupt) for wrong-type JSON, got {other:?}"),
        }
    }

    #[test]
    fn missing_budget_field_returns_corrupt_not_unlimited() {
        // `budget` is a safety control with NO serde default: a file that omits
        // it (manual edit / tamper / bad migration) must fail closed, NOT load as
        // a silent "unlimited" budget that still passes the onboarding gate.
        let (store, _dir) = temp_store();
        std::fs::write(
            &store.path,
            br#"{"onboarding_completed": true, "recorder_adapter": "sqlite"}"#,
        )
        .expect("seed budget-less file");
        match store.load() {
            Err(SettingsError::Corrupt) => {} // correct: missing budget → fail-closed
            other => panic!("expected Err(Corrupt) for a file missing budget, got {other:?}"),
        }
    }

    // ---- Load-path semantic validation (tampered but valid-JSON files) ----

    #[test]
    fn load_rejects_out_of_range_enabled_budget() {
        // enabled=true with a u64::MAX limit (hand-edited) must fail closed — the
        // save-path bound alone would let this load as a near-unlimited cap.
        let (store, _dir) = temp_store();
        std::fs::write(
            &store.path,
            br#"{"onboarding_completed":true,"budget":{"enabled":true,"monthly_limit_nanodollars":18446744073709551615},"recorder_adapter":"sqlite"}"#,
        )
        .expect("seed");
        assert!(matches!(store.load(), Err(SettingsError::Corrupt)));
    }

    #[test]
    fn load_rejects_disabled_budget_with_nonzero_limit() {
        let (store, _dir) = temp_store();
        std::fs::write(
            &store.path,
            br#"{"onboarding_completed":true,"budget":{"enabled":false,"monthly_limit_nanodollars":999},"recorder_adapter":"sqlite"}"#,
        )
        .expect("seed");
        assert!(matches!(store.load(), Err(SettingsError::Corrupt)));
    }

    #[test]
    fn load_rejects_unknown_recorder_adapter() {
        let (store, _dir) = temp_store();
        std::fs::write(
            &store.path,
            br#"{"onboarding_completed":true,"budget":{"enabled":false,"monthly_limit_nanodollars":0},"recorder_adapter":"obsidian"}"#,
        )
        .expect("seed");
        assert!(matches!(store.load(), Err(SettingsError::Corrupt)));
    }

    #[test]
    fn load_accepts_in_range_enabled_budget() {
        // $500 = 500e9 nano, enabled, sqlite → within bounds, loads fine.
        let (store, _dir) = temp_store();
        std::fs::write(
            &store.path,
            br#"{"onboarding_completed":true,"budget":{"enabled":true,"monthly_limit_nanodollars":500000000000},"recorder_adapter":"sqlite"}"#,
        )
        .expect("seed");
        let s = store.load().expect("valid in-range file loads");
        assert!(s.budget.enabled && s.onboarding_completed);
    }

    // ---- Atomic write leaves no partial file ------------------------------

    #[test]
    fn atomic_save_no_tmp_file_remains() {
        let (store, _dir) = temp_store();
        let settings = AppSettings::default();
        store.save(&settings).expect("save");

        // After a successful save, the .tmp sibling must not exist.
        let tmp = store.path.with_extension("json.tmp");
        assert!(
            !tmp.exists(),
            "tmp file should be renamed away after atomic save"
        );

        // The real file must exist.
        assert!(store.path.exists(), "settings file should exist after save");
    }

    // ---- Budget validation (build_budget_config) --------------------------

    #[test]
    fn budget_enabled_with_valid_usd_stores_nanodollars() {
        let config = build_budget_config(true, Some(10.0)).expect("valid");
        assert!(config.enabled);
        assert_eq!(config.monthly_limit_nanodollars, 10 * crate::cost_tracker::NANODOLLARS_PER_USD);
    }

    #[test]
    fn budget_enabled_none_usd_is_err() {
        assert!(build_budget_config(true, None).is_err());
    }

    #[test]
    fn budget_enabled_nan_is_err() {
        assert!(build_budget_config(true, Some(f64::NAN)).is_err());
    }

    #[test]
    fn budget_enabled_negative_is_err() {
        assert!(build_budget_config(true, Some(-1.0)).is_err());
    }

    #[test]
    fn budget_enabled_overflow_is_err() {
        assert!(build_budget_config(true, Some(1.0e30)).is_err());
    }

    #[test]
    fn budget_enabled_above_max_is_err() {
        // The Rust ceiling is authoritative — a direct IPC bypassing the UI's
        // <=1_000_000 guard must still be rejected.
        assert!(build_budget_config(true, Some(MAX_MONTHLY_LIMIT_USD + 1.0)).is_err());
        assert!(build_budget_config(true, Some(1.0e10)).is_err());
        // The boundary value itself is accepted.
        assert!(build_budget_config(true, Some(MAX_MONTHLY_LIMIT_USD)).is_ok());
    }

    #[test]
    fn budget_enabled_zero_or_negative_is_err() {
        // An enabled cap of 0 is degenerate (blocks everything immediately) and
        // is rejected; the UI requires > 0 too.
        assert!(build_budget_config(true, Some(0.0)).is_err());
        assert!(build_budget_config(true, Some(-5.0)).is_err());
    }

    #[test]
    fn budget_disabled_stores_unlimited() {
        let config = build_budget_config(false, None).expect("disabled unlimited");
        assert!(!config.enabled);
        assert_eq!(config.monthly_limit_nanodollars, 0);
    }

    #[test]
    fn budget_disabled_ignores_usd_value() {
        // When !enabled, the USD value is ignored (not validated).
        let config = build_budget_config(false, Some(99.0)).expect("disabled with value ignored");
        assert!(!config.enabled);
    }

    // ---- Adapter validation -----------------------------------------------

    #[test]
    fn validate_sqlite_adapter_ok() {
        assert!(validate_recorder_adapter("sqlite").is_ok());
    }

    #[test]
    fn validate_unknown_adapter_err() {
        assert!(validate_recorder_adapter("obsidian").is_err());
        assert!(validate_recorder_adapter("").is_err());
    }

    // ---- Multi-provider settings (koe-31u) --------------------------------

    #[test]
    fn load_migrates_file_without_voice_or_tool_fields() {
        // An already-onboarded user's file predates koe-31u: no
        // voice_provider_model, no tool_providers. It MUST load with the new
        // fields defaulted (not fail), or the migration would brick the app.
        let (store, _dir) = temp_store();
        std::fs::write(
            &store.path,
            br#"{"onboarding_completed":true,"budget":{"enabled":false,"monthly_limit_nanodollars":0},"recorder_adapter":"sqlite"}"#,
        )
        .expect("seed legacy file");
        let s = store.load().expect("legacy file migrates silently");
        assert_eq!(s.voice_provider_model, "openai/gpt-realtime-2");
        assert_eq!(s.tool_providers, ToolProviderFlags::default());
    }

    #[test]
    fn load_rejects_unknown_voice_provider() {
        let (store, _dir) = temp_store();
        std::fs::write(
            &store.path,
            br#"{"onboarding_completed":true,"budget":{"enabled":false,"monthly_limit_nanodollars":0},"recorder_adapter":"sqlite","voice_provider_model":"evil/model"}"#,
        )
        .expect("seed");
        assert!(matches!(store.load(), Err(SettingsError::Corrupt)));
    }

    #[test]
    fn load_accepts_known_voice_and_tool_flags() {
        let (store, _dir) = temp_store();
        std::fs::write(
            &store.path,
            br#"{"onboarding_completed":true,"budget":{"enabled":false,"monthly_limit_nanodollars":0},"recorder_adapter":"sqlite","voice_provider_model":"google/gemini-2.5-flash-live","tool_providers":{"xai":true,"x":false,"search":true}}"#,
        )
        .expect("seed");
        let s = store.load().expect("valid file loads");
        assert_eq!(s.voice_provider_model, "google/gemini-2.5-flash-live");
        assert!(s.tool_providers.xai && !s.tool_providers.x && s.tool_providers.search);
    }

    #[test]
    fn validate_voice_provider_model_allows_known_rejects_unknown() {
        assert!(validate_voice_provider_model("openai/gpt-realtime-2").is_ok());
        assert!(validate_voice_provider_model("google/gemini-2.5-flash-live").is_ok());
        assert!(validate_voice_provider_model("openai/gpt-4o").is_err());
        assert!(validate_voice_provider_model("openai").is_err());
        assert!(validate_voice_provider_model("").is_err());
    }

    #[test]
    fn tool_provider_flags_default_all_false() {
        let f = ToolProviderFlags::default();
        assert!(!f.xai && !f.x && !f.search);
    }

    #[test]
    fn tool_provider_flag_update_preserves_other_settings() {
        // Mirrors set_tool_provider_enabled's load-modify-write: flipping one flag
        // must not disturb budget / recorder / voice / the other flags.
        let (store, _dir) = temp_store();
        let base = AppSettings {
            onboarding_completed: true,
            budget: BudgetConfig {
                enabled: true,
                monthly_limit_nanodollars: 25_000_000_000,
            },
            recorder_adapter: "sqlite".into(),
            voice_provider_model: "openai/gpt-realtime-2".into(),
            tool_providers: ToolProviderFlags::default(),
            permission_policy: PermissionPolicy::default(),
        };
        store.save(&base).expect("seed");
        let mut current = store.load().expect("load");
        current.tool_providers.x = true; // flip exactly one
        store.save(&current).expect("save");
        let reloaded = store.load().expect("reload");
        assert!(reloaded.tool_providers.x);
        assert!(!reloaded.tool_providers.xai && !reloaded.tool_providers.search);
        assert_eq!(reloaded.budget, base.budget);
        assert_eq!(reloaded.recorder_adapter, "sqlite");
        assert_eq!(reloaded.voice_provider_model, "openai/gpt-realtime-2");
    }

    #[test]
    fn managed_update_persists_and_skips_save_on_err() {
        // ManagedSettings::update applies + persists on Ok; on a closure Err it
        // leaves the on-disk file unchanged (no partial write) — the guard that
        // set_tool_provider_enabled relies on for an unknown provider. The write
        // lock also serialises concurrent load-modify-save so rapid tool toggles
        // can't lose each other's updates (R-C / Codex High).
        let dir = tempfile::tempdir().expect("tempdir");
        let store: Arc<dyn SettingsStore> =
            Arc::new(JsonSettingsStore::new(dir.path().join("koe-settings.json")));
        store.save(&AppSettings::default()).expect("seed");
        let managed = ManagedSettings::new(Arc::clone(&store));

        managed
            .update(|s| {
                s.voice_provider_model = "google/gemini-2.5-flash-live".into();
                Ok(())
            })
            .expect("update ok");
        assert_eq!(
            store.load().expect("load").voice_provider_model,
            "google/gemini-2.5-flash-live"
        );

        let before = store.load().expect("load");
        let res = managed.update(|s| {
            s.tool_providers.xai = true; // mutate the in-memory copy …
            Err("rejected".to_string()) // … then reject → must NOT save
        });
        assert!(res.is_err());
        assert_eq!(
            store.load().expect("load"),
            before,
            "a failed update must not persist a partial change"
        );
    }

    #[test]
    fn tool_provider_key_name_maps_and_rejects() {
        assert_eq!(tool_provider_key_name("xai").unwrap(), XAI_KEY_NAME);
        assert_eq!(tool_provider_key_name("x").unwrap(), X_API_KEY_NAME);
        assert_eq!(tool_provider_key_name("search").unwrap(), SEARCH_KEY_NAME);
        // Voice providers are not togglable as tools, and unknown ids fail closed.
        assert!(tool_provider_key_name("openai").is_err());
        assert!(tool_provider_key_name("google").is_err());
        assert!(tool_provider_key_name("").is_err());
    }

    #[test]
    fn set_tool_flag_flips_only_target() {
        let mut f = ToolProviderFlags::default();
        set_tool_flag(&mut f, "x", true);
        assert!(f.x && !f.xai && !f.search);
        set_tool_flag(&mut f, "xai", true);
        assert!(f.x && f.xai && !f.search);
        set_tool_flag(&mut f, "x", false);
        assert!(!f.x && f.xai && !f.search);
    }

    // ---- Structural guard: settings commands are registered in lib.rs ------

    fn lib_rs_code_only() -> String {
        include_str!("lib.rs")
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn lib_rs_registers_settings_commands() {
        let code = lib_rs_code_only();
        for cmd in [
            "get_app_settings",
            "complete_onboarding",
            "save_budget_config",
            "set_recorder_adapter",
            "set_voice_provider",
            "set_tool_provider_enabled",
            "delete_tool_provider_key",
            "set_permission_policy", // koe-351
            "pick_folder",           // koe-351 folder picker
        ] {
            assert!(
                code.contains(cmd),
                "lib.rs must register command '{cmd}' in invoke_handler"
            );
        }
    }

    #[test]
    fn lib_rs_does_not_contain_greet() {
        let code = lib_rs_code_only();
        assert!(
            !code.contains("greet"),
            "greet scaffold command must be removed from lib.rs"
        );
    }

    // ---- Permission policy (koe-351) --------------------------------------

    #[test]
    fn load_migrates_file_without_permission_policy() {
        // A file predating koe-351 (no permission_policy) must load with an empty
        // policy (auto-approve nothing), not fail.
        let (store, _dir) = temp_store();
        std::fs::write(
            &store.path,
            br#"{"onboarding_completed":true,"budget":{"enabled":false,"monthly_limit_nanodollars":0},"recorder_adapter":"sqlite"}"#,
        )
        .expect("seed legacy file");
        let s = store.load().expect("legacy file migrates");
        assert_eq!(s.permission_policy, PermissionPolicy::default());
    }

    #[test]
    fn load_rejects_over_cap_permission_policy() {
        // A tampered file with an over-cap deny list fails closed on load.
        let (store, _dir) = temp_store();
        let huge: Vec<String> = (0..300).map(|i| format!("/x/{i}")).collect();
        let s = AppSettings {
            permission_policy: PermissionPolicy {
                denied_folders: huge,
                ..Default::default()
            },
            ..AppSettings::default()
        };
        // Write the over-cap policy directly (bypassing the validating command).
        let json = serde_json::to_vec(&s).unwrap();
        std::fs::write(&store.path, json).unwrap();
        assert!(matches!(store.load(), Err(SettingsError::Corrupt)));
    }

    #[test]
    fn load_rejects_malformed_host_in_permission_policy() {
        let (store, _dir) = temp_store();
        let s = AppSettings {
            permission_policy: PermissionPolicy {
                denied_url_hosts: vec!["bad/host".into()],
                ..Default::default()
            },
            ..AppSettings::default()
        };
        let json = serde_json::to_vec(&s).unwrap();
        std::fs::write(&store.path, json).unwrap();
        assert!(matches!(store.load(), Err(SettingsError::Corrupt)));
    }

    #[test]
    fn settings_policy_provider_loaded_and_unavailable() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("koe-settings.json");
        let store: Arc<dyn SettingsStore> = Arc::new(JsonSettingsStore::new(path.clone()));
        let policy = PermissionPolicy {
            allowed_url_hosts: vec!["openai.com".into()],
            ..Default::default()
        };
        store
            .save(&AppSettings {
                permission_policy: policy.clone(),
                ..AppSettings::default()
            })
            .expect("seed");

        let provider = SettingsPolicyProvider(Arc::clone(&store));
        assert_eq!(provider.current_policy(), PolicyState::Loaded(policy));

        // A corrupt file → Unavailable (NOT an empty Loaded policy), so the
        // dispatcher keeps forcing approval for policy-relevant targets.
        std::fs::write(&path, b"} not json {").unwrap();
        assert_eq!(provider.current_policy(), PolicyState::Unavailable);
    }

    #[test]
    fn default_app_settings_has_empty_policy() {
        assert_eq!(
            AppSettings::default().permission_policy,
            PermissionPolicy::default()
        );
    }
}
