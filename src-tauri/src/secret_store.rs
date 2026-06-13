//! Secret store for the OpenAI API key (BYOK).
//!
//! # Security Day 0 invariants (see CLAUDE.md)
//!
//! - The API key lives **only** inside the Rust process. It is persisted in an
//!   IOTA Stronghold encrypted snapshot whose decryption key ("snapshot
//!   password") is itself stored in the OS keychain — never on disk in plain.
//! - The key value is **never** exposed back to the WebView: there is no
//!   `get_openai_api_key` Tauri command. Only `set` / `has` / `delete` are
//!   exposed. The internal `get_api_key` is for Rust callers (session_manager).
//! - [`SecretString`] redacts itself in `Debug` and is intentionally **not**
//!   `Serialize` / `Display`, so it cannot leak into a Tauri event payload,
//!   a `log` line, or a panic message.
//! - All error paths are **fail-closed**: corruption / decrypt failure / a
//!   missing keychain entry return `Err`, never a silent "false" or empty key.
//!
//! transaction N/A · idempotency_key N/A (encrypted-at-rest secret store, not billing)

use std::fmt;
use std::path::PathBuf;

use tauri_plugin_stronghold::stronghold::Stronghold;
use zeroize::Zeroizing;

/// Stronghold snapshot decryption key length. The IOTA `KeyProvider` requires
/// exactly 32 bytes (see plugin `kdf.rs`: `HASH_LENGTH = 32`).
const SNAPSHOT_KEY_LEN: usize = 32;

/// Client path inside the Stronghold snapshot. Stable across versions so the
/// snapshot keeps resolving after upgrades — INTENTIONALLY retained as
/// `koe-secrets` through the koe→Rhanis rename. This is an invisible internal
/// storage-partition identifier (not branding); renaming it would return
/// `ClientDataNotPresent` for existing snapshots and orphan every saved API key.
/// See docs/reviews/2026-06-13-rhanis-migration-plan.md (intentionally-kept list).
const CLIENT_PATH: &[u8] = b"koe-secrets";

/// Logical name of the OpenAI key record inside the store.
pub const OPENAI_KEY_NAME: &str = "openai_api_key";

// Sibling provider key records (rhanis-31u multi-provider foundation). OpenAI is
// intentionally left BARE ("openai_api_key") so the existing stored key keeps
// resolving — `session_manager` reads `get_api_key(OPENAI_KEY_NAME)` and a rename
// would orphan it. New providers are namespaced with a `voice.` / `tool.` prefix
// so a typo can never collide with the legacy bare name and the role of each
// record is visible at a glance. All records share the one `CLIENT_PATH` /
// snapshot / keychain key; they differ only by this byte-key. These are consumed
// later (rhanis-zv3 reads the voice key, rhanis-eal reads the 手足 tool keys); for now
// they exist so the namespacing primitive ([`provider_key_name`]) is complete.
/// Voice provider = Google (Gemini Live). Consumed later by rhanis-zv3.
pub const GOOGLE_KEY_NAME: &str = "voice.google_api_key";
/// 手足 tool key: XAI (Grok). Consumed later by rhanis-eal.
pub const XAI_KEY_NAME: &str = "tool.xai_api_key";
/// 手足 tool key: X (Twitter) API. Consumed later by rhanis-eal.
pub const X_API_KEY_NAME: &str = "tool.x_api_key";
/// 手足 tool key: search provider. Consumed later by rhanis-8fw / rhanis-eal.
pub const SEARCH_KEY_NAME: &str = "tool.search_api_key";

// ---------------------------------------------------------------------------
// SecretString — a redacted, non-serializable string wrapper.
// ---------------------------------------------------------------------------

/// A string whose contents are zeroized on drop and never printed, logged, or
/// serialized. Use [`SecretString::expose`] only at the exact call site that
/// needs the raw value (e.g. building the `Authorization` header).
pub struct SecretString(Zeroizing<String>);

impl SecretString {
    pub fn new(value: String) -> Self {
        Self(Zeroizing::new(value))
    }

    /// Returns the raw secret. Call sites must not log / serialize the result.
    pub fn expose(&self) -> &str {
        self.0.as_str()
    }
}

impl From<String> for SecretString {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

// Intentionally NO `Display`, NO `Serialize`, NO `Clone`. `Debug` is redacted.
impl fmt::Debug for SecretString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretString(***)")
    }
}

// ---------------------------------------------------------------------------
// Errors — fixed messages only, never echo the underlying cause.
// ---------------------------------------------------------------------------

/// Error returned by the secret store. `Display` returns a **fixed** message
/// per variant so that no path, key material, or backend detail can leak into
/// a Tauri command's `Result<_, String>`, a log line, or a panic.
#[derive(Debug, PartialEq, Eq)]
pub enum SecretError {
    /// No secret stored under the requested name. Returned by the internal
    /// `get_api_key`; session_manager (rhanis-e3m) matches on it. `#[allow(dead_code)]`
    /// until that consumer lands (part of the API contract, not skeleton).
    #[allow(dead_code)]
    NotFound,
    /// The store could not be opened/decrypted (wrong key / corrupt snapshot).
    Locked,
    /// The underlying backend (keychain / stronghold / RNG) failed.
    Backend,
}

impl fmt::Display for SecretError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let msg = match self {
            SecretError::NotFound => "secret not found",
            SecretError::Locked => "secret store is locked",
            SecretError::Backend => "secret store is unavailable",
        };
        f.write_str(msg)
    }
}

impl std::error::Error for SecretError {}

// ---------------------------------------------------------------------------
// SecretStore trait + SnapshotPassword provider (split for M2 portability).
// ---------------------------------------------------------------------------

/// Abstraction over the secret backend. M1 uses [`StrongholdSecretStore`]; M2
/// can swap in a Windows Credential Manager / macOS Keychain implementation
/// without touching callers.
pub trait SecretStore: Send + Sync {
    fn save_api_key(&self, name: &str, key: SecretString) -> Result<(), SecretError>;
    /// Internal-only: must never be wired to a Tauri command. Consumed by the
    /// session_manager (rhanis-e3m) to build the WebSocket `Authorization` header;
    /// `#[allow(dead_code)]` until that PR lands (interface, not skeleton).
    #[allow(dead_code)]
    fn get_api_key(&self, name: &str) -> Result<SecretString, SecretError>;
    fn delete_api_key(&self, name: &str) -> Result<(), SecretError>;
    fn has_api_key(&self, name: &str) -> Result<bool, SecretError>;
}

/// Provides the 32-byte Stronghold snapshot decryption key. Split from the
/// store so the key-source (OS keychain vs. test fixture) can vary independently
/// — this is the seam M2 secret backends will reuse.
///
/// Generation (`obtain_or_create`) is intentionally separate from lookup
/// (`obtain_existing`): the store only ever generates a fresh key when no
/// snapshot exists yet, so a missing keychain entry next to an existing
/// snapshot fails closed instead of silently orphaning the encrypted data.
pub trait SnapshotPassword: Send + Sync {
    /// Returns the stored key, or `None` if none has been created yet.
    /// Backend failures (other than "absent") are `Err`.
    fn obtain_existing(&self) -> Result<Option<Zeroizing<Vec<u8>>>, SecretError>;
    /// Returns the stored key, generating and persisting a new one if absent.
    fn obtain_or_create(&self) -> Result<Zeroizing<Vec<u8>>, SecretError>;
}

// ---------------------------------------------------------------------------
// KeychainPassword — get-or-generate a random 32-byte key in the OS keychain.
// ---------------------------------------------------------------------------

/// Stores the Stronghold snapshot key in the OS keychain (Windows Credential
/// Manager / macOS Keychain / secret-service). The key is generated with a
/// CSPRNG on first run; every failure path is fail-closed.
pub struct KeychainPassword {
    service: String,
    account: String,
}

impl KeychainPassword {
    pub fn new(service: impl Into<String>, account: impl Into<String>) -> Self {
        Self {
            service: service.into(),
            account: account.into(),
        }
    }
}

impl KeychainPassword {
    fn entry(&self) -> Result<keyring::Entry, SecretError> {
        keyring::Entry::new(&self.service, &self.account).map_err(|_| SecretError::Backend)
    }
}

impl SnapshotPassword for KeychainPassword {
    fn obtain_existing(&self) -> Result<Option<Zeroizing<Vec<u8>>>, SecretError> {
        match self.entry()?.get_secret() {
            Ok(bytes) => {
                if bytes.len() == SNAPSHOT_KEY_LEN {
                    Ok(Some(Zeroizing::new(bytes)))
                } else {
                    // Wrong length = tampered / corrupt entry. Fail closed
                    // rather than treating it as absent (which could trigger a
                    // regenerate that orphans the existing encrypted snapshot).
                    Err(SecretError::Backend)
                }
            }
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(_) => Err(SecretError::Backend),
        }
    }

    fn obtain_or_create(&self) -> Result<Zeroizing<Vec<u8>>, SecretError> {
        if let Some(key) = self.obtain_existing()? {
            return Ok(key);
        }
        let mut key = Zeroizing::new(vec![0u8; SNAPSHOT_KEY_LEN]);
        getrandom::getrandom(key.as_mut_slice()).map_err(|_| SecretError::Backend)?;
        self.entry()?
            .set_secret(&key)
            .map_err(|_| SecretError::Backend)?;
        Ok(key)
    }
}

// ---------------------------------------------------------------------------
// StrongholdSecretStore — the real M1 implementation.
// ---------------------------------------------------------------------------

/// Persists secrets in an encrypted Stronghold snapshot. The snapshot is opened
/// per operation (open → op → save → drop) so decrypted state never lingers in
/// memory between calls.
///
/// NOTE: the `tauri-plugin-stronghold` *plugin* is deliberately **not**
/// registered with the Tauri builder. We use only the `Stronghold` wrapper
/// type, so zero stronghold JavaScript commands exist and the WebView has no
/// way to reach the vault (see [`module docs`](self)).
pub struct StrongholdSecretStore {
    snapshot_path: PathBuf,
    password: Box<dyn SnapshotPassword>,
    /// Serializes the whole `obtain → open → op → save` sequence. Tauri spawns a
    /// task per `invoke`, so without this two concurrent first-run calls could
    /// each generate a different snapshot key and race their snapshot saves,
    /// leaving the keychain key and the snapshot's encryption key mismatched
    /// (vault permanently locked). Operations are synchronous, so a std Mutex is
    /// held only for the brief op duration — never across an `.await`.
    lock: std::sync::Mutex<()>,
}

impl StrongholdSecretStore {
    pub fn new(snapshot_path: PathBuf, password: Box<dyn SnapshotPassword>) -> Self {
        Self {
            snapshot_path,
            password,
            lock: std::sync::Mutex::new(()),
        }
    }

    fn snapshot_exists(&self) -> bool {
        self.snapshot_path.exists()
    }

    /// Opens the snapshot for a **read** op. Returns `None` when nothing has ever
    /// been stored (no key + no snapshot). A missing key *next to* an existing
    /// snapshot, or an undecryptable snapshot, fails closed (`Locked`).
    fn open_read(&self) -> Result<Option<Stronghold>, SecretError> {
        match self.password.obtain_existing()? {
            None => {
                if self.snapshot_exists() {
                    // Snapshot present but its key is gone — unrecoverable, and
                    // never silently "empty".
                    Err(SecretError::Locked)
                } else {
                    Ok(None)
                }
            }
            Some(pw) => Ok(Some(self.open_with(pw)?)),
        }
    }

    /// Opens the snapshot for a **write** op, creating the snapshot key only when
    /// no snapshot exists yet (first-time setup).
    fn open_write(&self) -> Result<Stronghold, SecretError> {
        let pw = if self.snapshot_exists() {
            // Reuse the existing key; generating a new one would orphan the
            // snapshot's encrypted contents.
            self.password
                .obtain_existing()?
                .ok_or(SecretError::Locked)?
        } else {
            self.password.obtain_or_create()?
        };
        self.open_with(pw)
    }

    fn open_with(&self, pw: Zeroizing<Vec<u8>>) -> Result<Stronghold, SecretError> {
        Stronghold::new(&self.snapshot_path, pw.to_vec()).map_err(|_| SecretError::Locked)
    }

    /// Loads the client, distinguishing "no client yet" (empty store) from real
    /// backend failures (lock/restore/corrupt) which must surface as errors.
    fn load_client_opt(
        &self,
        stronghold: &Stronghold,
    ) -> Result<Option<iota_stronghold::Client>, SecretError> {
        match stronghold.load_client(CLIENT_PATH) {
            Ok(client) => Ok(Some(client)),
            Err(iota_stronghold::ClientError::ClientDataNotPresent) => Ok(None),
            // Any other error (lock failure, corrupt snapshot, …) is NOT "empty".
            Err(_) => Err(SecretError::Backend),
        }
    }
}

impl SecretStore for StrongholdSecretStore {
    fn save_api_key(&self, name: &str, key: SecretString) -> Result<(), SecretError> {
        let _guard = self.lock.lock().map_err(|_| SecretError::Backend)?;
        let stronghold = self.open_write()?;
        // First write creates the client; later writes reuse it.
        let client = match self.load_client_opt(&stronghold)? {
            Some(client) => client,
            None => stronghold
                .create_client(CLIENT_PATH)
                .map_err(|_| SecretError::Backend)?,
        };
        // `insert` returns any previous value; wrap it so a replaced key's
        // plaintext is zeroized rather than left lingering on the heap.
        let previous = client
            .store()
            .insert(
                name.as_bytes().to_vec(),
                key.expose().as_bytes().to_vec(),
                None,
            )
            .map_err(|_| SecretError::Backend)?;
        drop(previous.map(Zeroizing::new));
        // Persisting must succeed; a swallowed save would lose the key silently.
        stronghold.save().map_err(|_| SecretError::Backend)?;
        Ok(())
    }

    fn get_api_key(&self, name: &str) -> Result<SecretString, SecretError> {
        let _guard = self.lock.lock().map_err(|_| SecretError::Backend)?;
        let stronghold = match self.open_read()? {
            Some(stronghold) => stronghold,
            None => return Err(SecretError::NotFound),
        };
        let client = match self.load_client_opt(&stronghold)? {
            Some(client) => client,
            None => return Err(SecretError::NotFound),
        };
        match client
            .store()
            .get(name.as_bytes())
            .map_err(|_| SecretError::Backend)?
        {
            Some(bytes) => String::from_utf8(bytes)
                .map(SecretString::new)
                .map_err(|e| {
                    // Zeroize the secret bytes even on the (corruption) decode-failure
                    // path — FromUtf8Error owns the original Vec<u8>.
                    drop(Zeroizing::new(e.into_bytes()));
                    SecretError::Backend
                }),
            None => Err(SecretError::NotFound),
        }
    }

    fn delete_api_key(&self, name: &str) -> Result<(), SecretError> {
        let _guard = self.lock.lock().map_err(|_| SecretError::Backend)?;
        let stronghold = match self.open_read()? {
            Some(stronghold) => stronghold,
            // Nothing persisted yet — deletion is a no-op success.
            None => return Ok(()),
        };
        let client = match self.load_client_opt(&stronghold)? {
            Some(client) => client,
            None => return Ok(()),
        };
        // Zeroize the removed value rather than dropping the plaintext as-is.
        let removed = client
            .store()
            .delete(name.as_bytes())
            .map_err(|_| SecretError::Backend)?;
        drop(removed.map(Zeroizing::new));
        stronghold.save().map_err(|_| SecretError::Backend)?;
        Ok(())
    }

    fn has_api_key(&self, name: &str) -> Result<bool, SecretError> {
        let _guard = self.lock.lock().map_err(|_| SecretError::Backend)?;
        // `open_read()` Err here means corrupt / wrong-key: propagate, do NOT
        // collapse to `false` (a silent false would hide a tampered vault).
        let stronghold = match self.open_read()? {
            Some(stronghold) => stronghold,
            None => return Ok(false),
        };
        let client = match self.load_client_opt(&stronghold)? {
            Some(client) => client,
            // No client yet = legitimately empty.
            None => return Ok(false),
        };
        // `contains_key` avoids materializing (and having to zeroize) the
        // plaintext value just to test presence.
        client
            .store()
            .contains_key(name.as_bytes())
            .map_err(|_| SecretError::Backend)
    }
}

// ---------------------------------------------------------------------------
// Managed state + Tauri commands (WebView surface).
// ---------------------------------------------------------------------------

/// Tauri managed-state wrapper around the active [`SecretStore`].
pub struct ManagedSecretStore(pub std::sync::Arc<dyn SecretStore>);

/// Stores the OpenAI API key. The plain `key: String` arrives once over IPC at
/// entry time (accepted BYOK tradeoff) and is immediately moved into the
/// encrypted store; it is never returned to the WebView afterwards.
#[tauri::command]
pub async fn set_openai_api_key(
    key: String,
    store: tauri::State<'_, ManagedSecretStore>,
) -> Result<(), String> {
    let key = normalize_api_key(&key).map_err(|e| e.to_string())?;
    store
        .0
        .save_api_key(OPENAI_KEY_NAME, SecretString::new(key.to_string()))
        .map_err(|e| e.to_string())
}

/// Normalizes a pasted API key: trims surrounding whitespace/newlines (common
/// when copied from a terminal or `.env`) and rejects an empty result. Storing
/// an untrimmed value would make `has` report success while the
/// `Authorization: Bearer <key>` header silently fails at session start.
fn normalize_api_key(raw: &str) -> Result<&str, &'static str> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        Err("api key must not be empty")
    } else {
        Ok(trimmed)
    }
}

/// Reports whether an OpenAI key is stored, **without** returning its value.
#[tauri::command]
pub async fn has_openai_api_key(
    store: tauri::State<'_, ManagedSecretStore>,
) -> Result<bool, String> {
    store
        .0
        .has_api_key(OPENAI_KEY_NAME)
        .map_err(|e| e.to_string())
}

/// Deletes the stored OpenAI key.
#[tauri::command]
pub async fn delete_openai_api_key(
    store: tauri::State<'_, ManagedSecretStore>,
) -> Result<(), String> {
    store
        .0
        .delete_api_key(OPENAI_KEY_NAME)
        .map_err(|e| e.to_string())
}

// NOTE: there is deliberately NO `get_openai_api_key` command. The raw key is
// read only by Rust internals via `SecretStore::get_api_key`. The test
// `lib_rs_does_not_expose_get_command` locks this in.

// ---------------------------------------------------------------------------
// Multi-provider key commands (rhanis-31u). Same Day-0 invariants as the OpenAI
// trio above — write + boolean presence only, no get-* anywhere.
// ---------------------------------------------------------------------------

/// Maps a WebView-supplied provider id to its **fixed** Stronghold record name.
///
/// This is the single namespacing primitive. A `provider: String` that crosses
/// the IPC boundary is attacker/model-influenced, so it is resolved through this
/// CLOSED allowlist *before* any store operation: an unknown id returns a fixed
/// error and never becomes an arbitrary record key, so the WebView cannot read,
/// write, or delete a namespace that isn't enumerated here. rhanis-zv3 / rhanis-eal
/// extend this by ADDING arms (and the matching `*_KEY_NAME` const), never by
/// accepting a free-form name. `"openai"` maps to the legacy bare record so the
/// existing key path is unchanged.
fn provider_key_name(provider: &str) -> Result<&'static str, SecretError> {
    match provider {
        "openai" => Ok(OPENAI_KEY_NAME),
        "google" => Ok(GOOGLE_KEY_NAME),
        "xai" => Ok(XAI_KEY_NAME),
        "x" => Ok(X_API_KEY_NAME),
        "search" => Ok(SEARCH_KEY_NAME),
        // Fixed error — never echo the (attacker-influenced) provider string back.
        _ => Err(SecretError::NotFound),
    }
}

/// Stores an API key for `provider` (声 = `openai` / `google`, 手足 = `xai` /
/// `x` / `search`). The provider id is resolved through the [`provider_key_name`]
/// allowlist, so an unknown id is rejected before the vault is touched. Like
/// [`set_openai_api_key`], the plain key arrives once over IPC and is immediately
/// moved into the encrypted store; it is never returned to the WebView.
#[tauri::command]
pub async fn set_provider_api_key(
    provider: String,
    key: String,
    store: tauri::State<'_, ManagedSecretStore>,
) -> Result<(), String> {
    let name = provider_key_name(&provider).map_err(|e| e.to_string())?;
    let key = normalize_api_key(&key).map_err(|e| e.to_string())?;
    store
        .0
        .save_api_key(name, SecretString::new(key.to_string()))
        .map_err(|e| e.to_string())
}

/// Reports whether a key is stored for `provider`, **without** returning its
/// value. An Err (locked / corrupt vault) propagates rather than collapsing to
/// `false`, preserving the fail-closed contract.
#[tauri::command]
pub async fn has_provider_api_key(
    provider: String,
    store: tauri::State<'_, ManagedSecretStore>,
) -> Result<bool, String> {
    let name = provider_key_name(&provider).map_err(|e| e.to_string())?;
    store.0.has_api_key(name).map_err(|e| e.to_string())
}

/// Deletes the stored key for `provider`.
#[tauri::command]
pub async fn delete_provider_api_key(
    provider: String,
    store: tauri::State<'_, ManagedSecretStore>,
) -> Result<(), String> {
    let name = provider_key_name(&provider).map_err(|e| e.to_string())?;
    store.0.delete_api_key(name).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Deterministic 32-byte password for tests (no keychain needed). Behaves
    /// like a keychain that already holds the key.
    struct FixedPassword(Zeroizing<Vec<u8>>);
    impl FixedPassword {
        fn new() -> Self {
            Self(Zeroizing::new(vec![7u8; SNAPSHOT_KEY_LEN]))
        }
    }
    impl SnapshotPassword for FixedPassword {
        fn obtain_existing(&self) -> Result<Option<Zeroizing<Vec<u8>>>, SecretError> {
            Ok(Some(Zeroizing::new(self.0.to_vec())))
        }
        fn obtain_or_create(&self) -> Result<Zeroizing<Vec<u8>>, SecretError> {
            Ok(Zeroizing::new(self.0.to_vec()))
        }
    }

    /// Simulates a keychain whose entry was deleted (no existing key), but which
    /// would generate a *different* key if asked to create one.
    struct NoKeyPassword;
    impl SnapshotPassword for NoKeyPassword {
        fn obtain_existing(&self) -> Result<Option<Zeroizing<Vec<u8>>>, SecretError> {
            Ok(None)
        }
        fn obtain_or_create(&self) -> Result<Zeroizing<Vec<u8>>, SecretError> {
            Ok(Zeroizing::new(vec![9u8; SNAPSHOT_KEY_LEN]))
        }
    }

    /// Generates a non-32-byte key, which `KeyProvider` must reject.
    struct WrongLenPassword;
    impl SnapshotPassword for WrongLenPassword {
        fn obtain_existing(&self) -> Result<Option<Zeroizing<Vec<u8>>>, SecretError> {
            Ok(None)
        }
        fn obtain_or_create(&self) -> Result<Zeroizing<Vec<u8>>, SecretError> {
            Ok(Zeroizing::new(vec![1u8; 16])) // not 32 -> KeyProvider rejects
        }
    }

    fn temp_store(
        password: Box<dyn SnapshotPassword>,
    ) -> (StrongholdSecretStore, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("koe-secrets.stronghold");
        (StrongholdSecretStore::new(path, password), dir)
    }

    // ---- SecretString redaction --------------------------------------------

    #[test]
    fn secret_string_debug_is_redacted() {
        let s = SecretString::new("sk-super-secret-value".to_string());
        let rendered = format!("{:?}", s);
        assert_eq!(rendered, "SecretString(***)");
        assert!(!rendered.contains("sk-super-secret-value"));
    }

    #[test]
    fn secret_string_exposes_raw_only_via_expose() {
        let s = SecretString::new("raw".to_string());
        assert_eq!(s.expose(), "raw");
    }

    // ---- API key normalization ---------------------------------------------

    #[test]
    fn normalize_api_key_trims_and_rejects_empty() {
        assert_eq!(normalize_api_key("  sk-abc123\n").unwrap(), "sk-abc123");
        assert_eq!(normalize_api_key("sk-x").unwrap(), "sk-x");
        assert_eq!(normalize_api_key("\t sk-y \r\n").unwrap(), "sk-y");
        assert!(normalize_api_key("   ").is_err());
        assert!(normalize_api_key("").is_err());
    }

    // ---- SecretError fixed messages ----------------------------------------

    #[test]
    fn secret_error_messages_are_fixed_and_leak_free() {
        assert_eq!(SecretError::NotFound.to_string(), "secret not found");
        assert_eq!(SecretError::Locked.to_string(), "secret store is locked");
        assert_eq!(
            SecretError::Backend.to_string(),
            "secret store is unavailable"
        );
    }

    // ---- Stronghold round trip (real backend, fixed password, tempfile) ----

    #[test]
    fn save_then_get_round_trips() {
        let (store, _dir) = temp_store(Box::new(FixedPassword::new()));
        store
            .save_api_key("openai", SecretString::new("sk-abc123".to_string()))
            .expect("save");
        let got = store.get_api_key("openai").expect("get");
        assert_eq!(got.expose(), "sk-abc123");
    }

    #[test]
    fn has_reflects_presence() {
        let (store, _dir) = temp_store(Box::new(FixedPassword::new()));
        assert!(!store.has_api_key("openai").expect("has before"));
        store
            .save_api_key("openai", SecretString::new("sk-x".to_string()))
            .expect("save");
        assert!(store.has_api_key("openai").expect("has after"));
    }

    #[test]
    fn get_missing_is_not_found() {
        let (store, _dir) = temp_store(Box::new(FixedPassword::new()));
        assert_eq!(
            store.get_api_key("openai").unwrap_err(),
            SecretError::NotFound
        );
    }

    #[test]
    fn delete_removes_key() {
        let (store, _dir) = temp_store(Box::new(FixedPassword::new()));
        store
            .save_api_key("openai", SecretString::new("sk-del".to_string()))
            .expect("save");
        store.delete_api_key("openai").expect("delete");
        assert!(!store.has_api_key("openai").expect("has"));
        assert_eq!(
            store.get_api_key("openai").unwrap_err(),
            SecretError::NotFound
        );
    }

    #[test]
    fn delete_when_empty_is_ok() {
        let (store, _dir) = temp_store(Box::new(FixedPassword::new()));
        store
            .delete_api_key("openai")
            .expect("delete on empty is no-op");
    }

    #[test]
    fn persists_across_reopen() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("koe-secrets.stronghold");
        {
            let store = StrongholdSecretStore::new(path.clone(), Box::new(FixedPassword::new()));
            store
                .save_api_key("openai", SecretString::new("sk-persist".to_string()))
                .expect("save");
        }
        // Fresh store instance, same snapshot + password.
        let store = StrongholdSecretStore::new(path, Box::new(FixedPassword::new()));
        assert_eq!(
            store.get_api_key("openai").expect("get").expose(),
            "sk-persist"
        );
    }

    #[test]
    fn wrong_length_password_fails_closed() {
        let (store, _dir) = temp_store(Box::new(WrongLenPassword));
        // KeyProvider requires exactly 32 bytes -> open_write -> Locked.
        match store.save_api_key("openai", SecretString::new("x".to_string())) {
            Err(e) => assert_eq!(e, SecretError::Locked),
            Ok(_) => panic!("expected Locked error for wrong-length password"),
        }
    }

    #[test]
    fn missing_key_with_existing_snapshot_fails_closed() {
        // Snapshot exists but the keychain key is gone: must NOT silently report
        // "empty" or regenerate a key (which would orphan the snapshot).
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("koe-secrets.stronghold");
        StrongholdSecretStore::new(path.clone(), Box::new(FixedPassword::new()))
            .save_api_key("openai", SecretString::new("sk-orphan".to_string()))
            .expect("seed snapshot");

        let store = StrongholdSecretStore::new(path, Box::new(NoKeyPassword));
        match store.has_api_key("openai") {
            Err(e) => assert_eq!(e, SecretError::Locked),
            Ok(v) => panic!("expected Locked, got Ok({v})"),
        }
    }

    // ---- Structural guards (Codex R-A) -------------------------------------

    #[test]
    fn capability_does_not_grant_stronghold_permission() {
        // The WebView must not be able to reach stronghold JS commands.
        let cap = include_str!("../capabilities/default.json");
        assert!(
            !cap.to_lowercase().contains("stronghold"),
            "capabilities/default.json must NOT grant any stronghold permission"
        );
    }

    /// lib.rs with `//` comment lines stripped, so documentation that *mentions*
    /// a forbidden pattern doesn't trip the structural guards below.
    fn lib_rs_code_only() -> String {
        include_str!("lib.rs")
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn lib_rs_does_not_expose_get_command() {
        // Lock in that the raw key has no WebView read path — for ANY provider,
        // not just OpenAI. A single-literal check would silently let a future
        // `get_xai_api_key` slip through, so forbid every `get_*_api_key` shape.
        let code = lib_rs_code_only();
        for forbidden in [
            "get_openai_api_key",
            "get_google_api_key",
            "get_xai_api_key",
            "get_x_api_key",
            "get_search_api_key",
            "get_provider_api_key",
        ] {
            assert!(
                !code.contains(forbidden),
                "{forbidden} must never be registered as a Tauri command (no WebView key read path)"
            );
        }
        // The write path stays wired (sanity that the guard is checking live code).
        assert!(
            code.contains("set_openai_api_key"),
            "set_openai_api_key should be wired into the invoke handler"
        );
    }

    // ---- Multi-provider key namespacing (rhanis-31u) --------------------------

    #[test]
    fn provider_key_name_maps_allowlisted_ids() {
        assert_eq!(provider_key_name("openai").unwrap(), OPENAI_KEY_NAME);
        assert_eq!(provider_key_name("google").unwrap(), GOOGLE_KEY_NAME);
        assert_eq!(provider_key_name("xai").unwrap(), XAI_KEY_NAME);
        assert_eq!(provider_key_name("x").unwrap(), X_API_KEY_NAME);
        assert_eq!(provider_key_name("search").unwrap(), SEARCH_KEY_NAME);
    }

    #[test]
    fn provider_key_name_rejects_unknown_without_echo() {
        // Unknown / attacker-supplied ids get a FIXED error and never map to a
        // record key — the error must not echo the provider string.
        for bad in ["", "OPENAI", "../escape", "voice.google_api_key", "anything"] {
            let err = provider_key_name(bad).unwrap_err();
            assert_eq!(err, SecretError::NotFound);
            assert!(
                !err.to_string().contains(bad) || bad.is_empty(),
                "error message must not echo the provider id"
            );
        }
    }

    #[test]
    fn provider_key_names_are_pairwise_distinct() {
        use std::collections::HashSet;
        let names = [
            OPENAI_KEY_NAME,
            GOOGLE_KEY_NAME,
            XAI_KEY_NAME,
            X_API_KEY_NAME,
            SEARCH_KEY_NAME,
        ];
        let set: HashSet<&str> = names.iter().copied().collect();
        assert_eq!(set.len(), names.len(), "key record names must be unique");
        // OpenAI stays bare (backward compat); the rest are namespaced.
        assert_eq!(OPENAI_KEY_NAME, "openai_api_key");
        for n in [GOOGLE_KEY_NAME] {
            assert!(n.starts_with("voice."), "{n} must be voice-namespaced");
        }
        for n in [XAI_KEY_NAME, X_API_KEY_NAME, SEARCH_KEY_NAME] {
            assert!(n.starts_with("tool."), "{n} must be tool-namespaced");
        }
    }

    #[test]
    fn openai_provider_resolves_to_the_existing_record() {
        // Backward-compat proof: storing under provider "openai" writes the SAME
        // record the legacy OpenAI path reads, so an already-onboarded user's key
        // keeps working without migration.
        let (store, _dir) = temp_store(Box::new(FixedPassword::new()));
        let name = provider_key_name("openai").unwrap();
        store
            .save_api_key(name, SecretString::new("sk-legacy".to_string()))
            .expect("save under provider 'openai'");
        // Read back via the original OPENAI_KEY_NAME record.
        assert!(store.has_api_key(OPENAI_KEY_NAME).expect("has openai"));
        assert_eq!(
            store.get_api_key(OPENAI_KEY_NAME).expect("get").expose(),
            "sk-legacy"
        );
    }

    #[test]
    fn provider_key_round_trips_independently() {
        // A 手足 provider's key set→has→delete→has cycle is independent of others.
        let (store, _dir) = temp_store(Box::new(FixedPassword::new()));
        let xai = provider_key_name("xai").unwrap();
        assert!(!store.has_api_key(xai).expect("has before"));
        store
            .save_api_key(xai, SecretString::new("xai-key".to_string()))
            .expect("save xai");
        assert!(store.has_api_key(xai).expect("has after save"));
        // A different provider remains absent (distinct record).
        assert!(!store
            .has_api_key(provider_key_name("x").unwrap())
            .expect("x absent"));
        store.delete_api_key(xai).expect("delete xai");
        assert!(!store.has_api_key(xai).expect("has after delete"));
    }

    #[test]
    fn has_provider_propagates_locked_fail_closed() {
        // A locked vault must surface as Err, not a silent `false` — same
        // fail-closed contract the OpenAI path has.
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("koe-secrets.stronghold");
        StrongholdSecretStore::new(path.clone(), Box::new(FixedPassword::new()))
            .save_api_key(provider_key_name("xai").unwrap(), SecretString::new("k".into()))
            .expect("seed snapshot");
        let store = StrongholdSecretStore::new(path, Box::new(NoKeyPassword));
        match store.has_api_key(provider_key_name("xai").unwrap()) {
            Err(e) => assert_eq!(e, SecretError::Locked),
            Ok(v) => panic!("expected Locked, got Ok({v})"),
        }
    }

    #[test]
    fn stronghold_plugin_is_not_registered() {
        // Registering the plugin would add stronghold JS commands; we must not.
        let code = lib_rs_code_only();
        assert!(
            !code.contains("stronghold::Builder"),
            "the stronghold plugin must not be registered (Rust-internal use only)"
        );
    }
}
