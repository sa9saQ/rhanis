mod approval_gate;
mod cost_tracker;
mod events;
mod realtime_types;
mod secret_store;
mod settings_store;
mod storage;
mod tool_dispatcher;
mod tools;
mod validation;

use std::sync::Arc;

use tauri::Manager;

use approval_gate::{resolve_tool_approval, ApprovalGate, ManagedApprovalGate};
use events::{ManagedSequenceCounter, SequenceCounter};
use secret_store::{
    delete_openai_api_key, has_openai_api_key, set_openai_api_key, KeychainPassword,
    ManagedSecretStore, StrongholdSecretStore,
};
use settings_store::{
    complete_onboarding, get_app_settings, save_budget_config, set_recorder_adapter,
    JsonSettingsStore, ManagedSettings,
};
use realtime_types::ManagedDispatcher;
use storage::{
    adapter::{ManagedRecorder, RecorderAdapter},
    sqlite::SqliteAdapter,
};
use tool_dispatcher::{AppDispatchIo, RealToolDispatcher, ToolRegistry};

/// Keychain identifiers for the Stronghold snapshot decryption key.
const KEYCHAIN_SERVICE: &str = "com.zsaku.koe";
const KEYCHAIN_SNAPSHOT_ACCOUNT: &str = "stronghold-snapshot-key";

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        // NOTE: the stronghold *plugin* is intentionally NOT registered. We use
        // only the `Stronghold` wrapper type from Rust (see secret_store.rs), so
        // no stronghold JavaScript commands exist and the WebView cannot reach
        // the vault. Do not add `.plugin(tauri_plugin_stronghold::Builder...)`.
        .setup(|app| {
            // Snapshot lives in the per-user app data dir; its decryption key
            // is held in the OS keychain (never on disk in plain).
            let data_dir = app.path().app_local_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            let snapshot_path = data_dir.join("koe-secrets.stronghold");

            let password = Box::new(KeychainPassword::new(
                KEYCHAIN_SERVICE,
                KEYCHAIN_SNAPSHOT_ACCOUNT,
            ));
            let store = StrongholdSecretStore::new(snapshot_path, password);
            app.manage(ManagedSecretStore(Arc::new(store)));

            // Recorder storage (koe-nnk): notes / conversation log / cost
            // snapshots in a Rust-owned SQLite DB beside the secret snapshot.
            // No WebView SQL surface; consumers (write_note tool koe-s7i,
            // session_manager koe-e3m) reach it via tauri::State<ManagedRecorder>.
            let recorder: Arc<dyn RecorderAdapter> =
                Arc::new(SqliteAdapter::open(&data_dir.join("koe.db"))?);
            app.manage(ManagedRecorder(Arc::clone(&recorder)));

            // Approval gate (koe-1vi). One process-wide activity-event sequence
            // is shared between the gate (ApprovalRequest.sequence) and the
            // future tool_dispatcher (koe-2gy, ToolEvent.sequence) — koe-2gy
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

            // Tool dispatcher (koe-2gy). Shares the one gate + sequence and emits
            // tool-events via the real AppHandle. write_note is registered now;
            // koe-s7i plugs the remaining tools into the same registry.
            let io = Arc::new(AppDispatchIo::new(app.handle().clone(), Arc::clone(&gate)));
            let mut registry = ToolRegistry::new();
            tools::register_m1_tools(&mut registry, Arc::clone(&recorder));
            let dispatcher = RealToolDispatcher::new(io, Arc::clone(&sequence), Arc::new(registry));
            app.manage(ManagedDispatcher(Arc::new(dispatcher)));

            // Settings persistence (koe-200): onboarding flag + budget config +
            // recorder adapter choice. Rust-owned JSON; no WebView file surface.
            let settings_path = data_dir.join("koe-settings.json");
            let settings = JsonSettingsStore::new(settings_path);
            app.manage(ManagedSettings(Arc::new(settings)));

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            set_openai_api_key,
            has_openai_api_key,
            delete_openai_api_key,
            resolve_tool_approval,
            get_app_settings,
            complete_onboarding,
            save_budget_config,
            set_recorder_adapter,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
