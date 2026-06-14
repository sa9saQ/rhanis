mod approval_gate;
mod audio_bridge;
mod cost_tracker;
mod display_descriptor;
mod events;
mod permission_policy;
mod realtime_provider;
mod realtime_types;
mod secret_store;
mod session_manager;
mod settings_store;
mod storage;
mod tool_dispatcher;
mod tools;
mod validation;

use std::sync::Arc;

use tauri::Manager;

use approval_gate::{resolve_tool_approval, ApprovalGate, ManagedApprovalGate};
use audio_bridge::ManagedAudioBridge;
use events::{ManagedSequenceCounter, SequenceCounter};
use secret_store::{
    delete_openai_api_key, delete_provider_api_key, has_openai_api_key, has_provider_api_key,
    set_openai_api_key, set_provider_api_key, KeychainPassword, ManagedSecretStore,
    StrongholdSecretStore,
};
use settings_store::{
    complete_onboarding, delete_tool_provider_key, get_app_settings, save_budget_config,
    set_permission_policy, set_recorder_adapter, set_tool_provider_enabled, set_voice_provider,
    JsonSettingsStore, ManagedSettings, SettingsPolicyProvider, SettingsStore,
};
use realtime_types::ManagedDispatcher;
use session_manager::{get_cost_snapshot, start_session, stop_session, ManagedSession};
use storage::{
    adapter::{ManagedRecorder, RecorderAdapter},
    sqlite::SqliteAdapter,
};
use tool_dispatcher::{AppDispatchIo, RealToolDispatcher, ToolRegistry};

/// Keychain identifiers for the Stronghold snapshot decryption key; uses the
/// bundle identifier `com.zsaku.rhanis`. (koe→Rhanis rename: koe was never
/// distributed, and the identifier change already moves `app_local_data_dir` to
/// a fresh directory, so no prior snapshot/key is preserved — pre-distribution,
/// a one-time dev re-entry of the BYOK key. See the migration plan.)
const KEYCHAIN_SERVICE: &str = "com.zsaku.rhanis";
const KEYCHAIN_SNAPSHOT_ACCOUNT: &str = "stronghold-snapshot-key";

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        // Dialog plugin (rhanis-351): registered so the Rust side can open a native
        // folder picker for the permission-policy editor. Reached ONLY through our
        // own `pick_folder` command (below) — the WebView is NOT granted any
        // `dialog:*` capability, so it gains no direct dialog command surface (same
        // minimal-surface posture as the disabled stronghold JS plugin).
        .plugin(tauri_plugin_dialog::init())
        // NOTE: the stronghold *plugin* is intentionally NOT registered. We use
        // only the `Stronghold` wrapper type from Rust (see secret_store.rs), so
        // no stronghold JavaScript commands exist and the WebView cannot reach
        // the vault. Do not add `.plugin(tauri_plugin_stronghold::Builder...)`.
        .setup(|app| {
            // Snapshot lives in the per-user app data dir; its decryption key
            // is held in the OS keychain (never on disk in plain).
            let data_dir = app.path().app_local_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            let snapshot_path = data_dir.join("rhanis-secrets.stronghold");

            let password = Box::new(KeychainPassword::new(
                KEYCHAIN_SERVICE,
                KEYCHAIN_SNAPSHOT_ACCOUNT,
            ));
            // rhanis-ds6: drop the Stronghold snapshot encrypt work factor to 0
            // before the store performs any open/save, eliminating the ≈1s of
            // scrypt each save/open otherwise pays. Sound only because the snapshot
            // key is a 32-byte CSPRNG key — see the invariant on
            // secret_store::SNAPSHOT_ENCRYPT_WORK_FACTOR. Non-fatal: on failure the
            // global keeps the safe, slow default, so we do not abort startup.
            let _ = secret_store::set_encrypt_work_factor_for_strong_key();
            let store = StrongholdSecretStore::new(snapshot_path, password);
            app.manage(ManagedSecretStore(Arc::new(store)));

            // Recorder storage (rhanis-nnk): notes / conversation log / cost
            // snapshots in a Rust-owned SQLite DB beside the secret snapshot.
            // No WebView SQL surface; consumers (write_note tool rhanis-s7i,
            // session_manager rhanis-e3m) reach it via tauri::State<ManagedRecorder>.
            let recorder: Arc<dyn RecorderAdapter> =
                Arc::new(SqliteAdapter::open(&data_dir.join("rhanis.db"))?);
            app.manage(ManagedRecorder(Arc::clone(&recorder)));

            // Approval gate (rhanis-1vi). One process-wide activity-event sequence
            // is shared between the gate (ApprovalRequest.sequence) and the
            // future tool_dispatcher (rhanis-2gy, ToolEvent.sequence) — rhanis-2gy
            // obtains it via tauri::State<ManagedSequenceCounter>, not by
            // importing the gate, so the two never grow divergent counters.
            let sequence = Arc::new(SequenceCounter::new());
            app.manage(ManagedSequenceCounter(Arc::clone(&sequence)));
            // ONE ApprovalGate Arc is shared by the `resolve_tool_approval`
            // command's state AND the dispatcher below, so a resolve reaches the
            // exact pending request the dispatcher is awaiting. Two separate
            // `Arc::new(ApprovalGate::new(..))` would split the pending map and
            // DANGER approvals would never resolve.
            let gate = Arc::new(ApprovalGate::new(Arc::clone(&sequence)));
            app.manage(ManagedApprovalGate(Arc::clone(&gate)));

            // Settings persistence (rhanis-200 + rhanis-351): onboarding flag + budget +
            // recorder + voice/tool selections + permission policy. Rust-owned
            // JSON; no WebView file surface. Built BEFORE the dispatcher so the
            // permission-policy provider can share the SAME store Arc (so a policy
            // edit via `set_permission_policy` is seen by the next dispatch).
            let settings_path = data_dir.join("rhanis-settings.json");
            let settings_store: Arc<dyn SettingsStore> =
                Arc::new(JsonSettingsStore::new(settings_path));
            app.manage(ManagedSettings::new(Arc::clone(&settings_store)));

            // Tool dispatcher (rhanis-2gy). Shares the one gate + sequence, emits
            // tool-events via the real AppHandle, and composes the user permission
            // policy (rhanis-351) read from the shared settings store on each dispatch.
            let io = Arc::new(AppDispatchIo::new(app.handle().clone(), Arc::clone(&gate)));
            let mut registry = ToolRegistry::new();
            tools::register_m1_tools(&mut registry, Arc::clone(&recorder));
            let policy = Arc::new(SettingsPolicyProvider(Arc::clone(&settings_store)));
            let dispatcher =
                RealToolDispatcher::new(io, Arc::clone(&sequence), Arc::new(registry), policy);
            app.manage(ManagedDispatcher(Arc::new(dispatcher)));

            // Realtime session (rhanis-e3m): the RealToolDispatcher managed above
            // (rhanis-2gy) is read by session_manager via tauri::State<ManagedDispatcher>.
            app.manage(ManagedSession::new());

            // Audio bridge (rhanis-flu): cpal mic capture + rodio playback.
            // Start/stop is driven by session_manager (start_session / stop_session)
            // via tauri::State<ManagedAudioBridge>. The bridge is idle until a
            // session begins; device open happens inside start_session.
            app.manage(ManagedAudioBridge::new());

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            set_openai_api_key,
            has_openai_api_key,
            delete_openai_api_key,
            set_provider_api_key,
            has_provider_api_key,
            delete_provider_api_key,
            resolve_tool_approval,
            get_app_settings,
            complete_onboarding,
            save_budget_config,
            set_recorder_adapter,
            set_voice_provider,
            set_tool_provider_enabled,
            delete_tool_provider_key,
            set_permission_policy,
            pick_folder,
            start_session,
            stop_session,
            get_cost_snapshot,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// Opens the OS native folder picker and returns the chosen absolute path (or
/// `None` if the user cancelled). rhanis-351: the permission-policy editor calls
/// this via `invoke("pick_folder")`. The dialog runs entirely Rust-side (the
/// WebView has no `dialog:*` capability); only the resulting path string crosses
/// the IPC boundary. A non-filesystem result (should not happen for a folder) is
/// reported as a fixed error and never silently dropped.
#[tauri::command]
async fn pick_folder(app: tauri::AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;

    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog().file().pick_folder(move |picked| {
        let _ = tx.send(picked);
    });
    let picked = rx
        .await
        .map_err(|_| "folder picker is unavailable".to_string())?;
    match picked {
        None => Ok(None),
        Some(file_path) => file_path
            .into_path()
            .map(|p| Some(p.to_string_lossy().into_owned()))
            .map_err(|_| "selected folder could not be resolved".to_string()),
    }
}
