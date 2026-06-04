//! Recorder storage abstraction (koe-nnk).
//!
//! koe persists three kinds of data locally: free-form **notes** (the
//! `write_note` tool / user memos), a **conversation log** (assistant/user
//! turns and tool milestones), and periodic **cost snapshots** (so the monthly
//! budget total survives an app restart instead of resetting to zero).
//!
//! All of that sits behind [`RecorderAdapter`] so the backend can be swapped
//! without touching callers: M1 uses SQLite ([`super::sqlite::SqliteAdapter`]),
//! M2 will add an Obsidian adapter, M3 a Notion one.
//!
//! # Security posture (see CLAUDE.md)
//! - The DB is owned entirely by Rust. There is **no** WebView SQL surface
//!   (`tauri-plugin-sql` is intentionally not used — the same reason the
//!   stronghold *plugin* is disabled in `secret_store.rs`). Callers reach the
//!   store only through this trait, never by executing SQL from the front-end.
//! - [`RecorderError`]'s `Display` returns **fixed** strings, so a DB path, a
//!   SQL fragment, or row contents can never leak into a Tauri command's
//!   `Result<_, String>`, a log line, or a panic message.
//!
//! transaction N/A · idempotency_key N/A (local note/log store, not billing).

use std::sync::Arc;

use serde::{Deserialize, Serialize};

/// Error returned by the recorder. `Display` is a **fixed** message per variant
/// so no path, SQL, or stored content leaks to the WebView (mirrors
/// `secret_store::SecretError`).
#[derive(Debug, PartialEq, Eq)]
pub enum RecorderError {
    /// The database could not be opened or migrated (corrupt file, I/O failure,
    /// unwritable directory).
    Open,
    /// A query failed: constraint, `SQLITE_BUSY`, type error, or a poisoned
    /// connection lock (a prior op panicked mid-lock). Always fail-closed —
    /// never a silent empty result.
    Db,
}

impl std::fmt::Display for RecorderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let msg = match self {
            RecorderError::Open => "recorder storage is unavailable",
            RecorderError::Db => "recorder operation failed",
        };
        f.write_str(msg)
    }
}

// Required so `SqliteAdapter::open(...)?` works in `lib.rs` setup(), whose
// closure returns `Result<_, Box<dyn std::error::Error>>`.
impl std::error::Error for RecorderError {}

/// A persisted note (output of the `write_note` tool or a user memo).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Note {
    pub id: i64,
    pub text: String,
    /// Backend epoch milliseconds at insertion time (display only).
    pub created_at: i64,
}

/// A logged conversation event: an assistant/user turn or a tool milestone.
/// `summary` is expected to be pre-redacted by the caller (no key / path / PII).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversationEvent {
    pub id: i64,
    pub role: String,
    pub kind: String,
    pub summary: String,
    /// Backend epoch milliseconds at insertion time.
    pub created_at: i64,
}

/// Pluggable recorder backend. Implementations must be `Send + Sync` so the
/// adapter can live in Tauri managed state and be shared across the async tool
/// tasks.
///
/// Methods whose only caller is a not-yet-merged module carry
/// `#[allow(dead_code)]` (with the consumer named) — they are the stable API
/// contract those modules will import, not skeleton. This mirrors
/// `secret_store::SecretStore::get_api_key`.
pub trait RecorderAdapter: Send + Sync {
    /// Persists a note, returning its row id. Consumed by the `write_note` tool
    /// (koe-s7i) and the settings health probe (koe-200).
    ///
    /// `text` is stored verbatim. Callers (the tool_dispatcher, koe-2gy) own
    /// redaction AND size-bounding of tool output before it reaches the store,
    /// so the byte cap lives in one place rather than drifting between layers.
    fn save_note(&self, text: &str) -> Result<i64, RecorderError>;

    /// Most-recent-first notes, capped at `limit` (a `limit` of 0 returns an
    /// empty list). Consumed by the notes view / health probe.
    ///
    /// Returns content-bearing [`Note`]s (which derive `Serialize`). Any future
    /// Tauri command that forwards these to the WebView MUST be reviewed for PII
    /// exposure — the store does not redact (cf. `secret_store::get_api_key`,
    /// which is deliberately kept off the command surface).
    fn list_recent_notes(&self, limit: u32) -> Result<Vec<Note>, RecorderError>;

    /// Appends a conversation event, returning its row id. Consumed by
    /// session_manager (koe-e3m). `summary` is stored verbatim — callers own
    /// redaction and size-bounding (see [`save_note`](RecorderAdapter::save_note)).
    fn log_conversation_event(
        &self,
        role: &str,
        kind: &str,
        summary: &str,
    ) -> Result<i64, RecorderError>;

    /// Most-recent-first conversation events, capped at `limit`. (koe-e3m)
    /// Returns content-bearing structs; the same PII-review caveat as
    /// [`list_recent_notes`](RecorderAdapter::list_recent_notes) applies to any
    /// command that exposes them.
    fn list_recent_events(&self, limit: u32) -> Result<Vec<ConversationEvent>, RecorderError>;

    /// Adds `delta_nanodollars` to the running monthly cost ledger for a month
    /// (`YYYYMM`) and returns the new accumulated total (koe-ixt). This is an
    /// **additive ledger**, not an absolute-total upsert: the monthly cost is the
    /// SUM of every session's per-`response.done` charge, so adding each delta
    ///   - sums two sessions' spend that overlap during a stop→start handover
    ///     (where an older session's read loop is still draining late usage),
    ///     instead of keeping only the max and losing one side's contribution;
    ///   - is order-independent, so a late / out-of-order write can never roll the
    ///     total backwards (undercount);
    ///   - lets session_manager (koe-e3m) gate the budget on the authoritative
    ///     **cross-session** total rather than a single session's stale local
    ///     baseline (so a handover cannot run a newer session fail-open).
    ///
    /// The add MUST saturate at `u64::MAX` (never wrap, and never via SQLite's
    /// `+`, which promotes an `i64` overflow to a `REAL` float and breaks the
    /// integer-only / NaN-free budget invariant). The `cost_tracker` domain layer
    /// guarantees `month_yyyymm` validity; the store persists it verbatim.
    /// Consumed by session_manager (koe-e3m).
    fn add_month_cost(
        &self,
        month_yyyymm: u32,
        delta_nanodollars: u64,
    ) -> Result<u64, RecorderError>;

    /// Loads the accumulated cost total for a month, or `None` if absent. (koe-e3m)
    fn load_cost_snapshot(&self, month_yyyymm: u32) -> Result<Option<u64>, RecorderError>;

    /// Read-only liveness probe: confirms the store is openable and queryable
    /// **without writing**. A write-probe would pollute the user's notes table
    /// (which `list_recent_notes` surfaces). Probes the `notes` table only (the
    /// primary user-visible table) — a liveness check, not a full multi-table
    /// integrity check. Consumed by the settings health indicator (koe-200),
    /// which adds the Tauri command that calls it.
    fn health_check(&self) -> Result<(), RecorderError>;
}

/// Tauri managed-state wrapper around the active recorder. The DB is opened and
/// migrated once at startup (`lib.rs` setup) and shared (`Arc`) with future
/// callers (the `write_note` tool koe-s7i, session_manager koe-e3m) via
/// `tauri::State<'_, ManagedRecorder>` — the same managed-state seam as
/// `secret_store::ManagedSecretStore`.
pub struct ManagedRecorder(pub Arc<dyn RecorderAdapter>);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recorder_error_messages_are_fixed_and_leak_free() {
        assert_eq!(
            RecorderError::Open.to_string(),
            "recorder storage is unavailable"
        );
        assert_eq!(RecorderError::Db.to_string(), "recorder operation failed");
        // No path separators, SQL keywords, or digits that could carry detail.
        for e in [RecorderError::Open, RecorderError::Db] {
            let s = e.to_string();
            assert!(!s.contains('/') && !s.contains('\\'));
            assert!(!s.to_lowercase().contains("select"));
            assert!(!s.chars().any(|c| c.is_ascii_digit()));
        }
    }

    #[test]
    fn recorder_error_is_std_error() {
        // Locks in the `impl std::error::Error` that lib.rs setup()'s `?` needs.
        let _boxed: Box<dyn std::error::Error> = Box::new(RecorderError::Db);
        let _as_ref: &dyn std::error::Error = &RecorderError::Open;
    }

    #[test]
    fn recorder_error_debug_has_no_detail() {
        // Debug prints only the variant name; never a path or SQL.
        assert_eq!(format!("{:?}", RecorderError::Open), "Open");
        assert_eq!(format!("{:?}", RecorderError::Db), "Db");
    }
}
