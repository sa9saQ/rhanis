mod cost_tracker;
mod secret_store;

use std::sync::Arc;

use tauri::Manager;

use secret_store::{
    delete_openai_api_key, has_openai_api_key, set_openai_api_key, KeychainPassword,
    ManagedSecretStore, StrongholdSecretStore,
};

/// Keychain identifiers for the Stronghold snapshot decryption key.
const KEYCHAIN_SERVICE: &str = "com.zsaku.koe";
const KEYCHAIN_SNAPSHOT_ACCOUNT: &str = "stronghold-snapshot-key";

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

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
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            greet,
            set_openai_api_key,
            has_openai_api_key,
            delete_openai_api_key
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
