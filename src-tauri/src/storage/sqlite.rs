//! SQLite-backed recorder (koe-nnk default).
//!
//! Uses `rusqlite` with the `bundled` feature so SQLite is compiled in — no
//! system library dependency, which keeps Windows builds reproducible. The
//! `Connection` is owned by Rust behind a `Mutex`; there is **no** WebView SQL
//! surface (we deliberately do not use `tauri-plugin-sql`, mirroring the
//! disabled stronghold plugin in `secret_store.rs`).
//!
//! Concurrency: every method takes the connection `Mutex` for the brief op
//! duration. The lock is a `std::sync::Mutex` and is never held across an
//! `.await` (the methods are synchronous; the Tauri command layer that will
//! consume them hops onto a blocking thread). A poisoned lock (a prior op
//! panicked) maps to `RecorderError::Db` — it never re-panics the process.

use std::path::Path;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, OptionalExtension};

use super::adapter::{ConversationEvent, Note, RecorderAdapter, RecorderError};

/// Wall-clock epoch milliseconds for `created_at`. Display-only ordering uses
/// the autoincrement `id`, so a clock skew here cannot reorder rows. A clock
/// before the epoch falls back to 0, and a far-future clock (past year ~292M)
/// saturates at `i64::MAX` rather than truncating via a raw `as` cast — both
/// stay display-only and never panic.
fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

/// SQLite implementation of [`RecorderAdapter`].
pub struct SqliteAdapter {
    conn: Mutex<Connection>,
}

impl SqliteAdapter {
    /// Opens (creating if absent) the DB at `path` and runs the idempotent
    /// migration. Any failure is `RecorderError::Open` (fail-closed; the caller
    /// in `lib.rs` setup propagates it).
    pub fn open(path: &Path) -> Result<Self, RecorderError> {
        let conn = Connection::open(path).map_err(|_| RecorderError::Open)?;
        Self::init(conn)
    }

    /// In-memory DB for tests.
    #[cfg(test)]
    fn open_in_memory() -> Result<Self, RecorderError> {
        let conn = Connection::open_in_memory().map_err(|_| RecorderError::Open)?;
        Self::init(conn)
    }

    fn init(conn: Connection) -> Result<Self, RecorderError> {
        // WAL lets a future read-only connection (koe-e3m diagnostics) avoid
        // SQLITE_BUSY against the writer. Best-effort: an in-memory DB reports
        // "memory" and ignores WAL, which must not fail open().
        let _ = conn.pragma_update(None, "journal_mode", "WAL");
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS notes (
                 id         INTEGER PRIMARY KEY AUTOINCREMENT,
                 text       TEXT    NOT NULL,
                 created_at INTEGER NOT NULL
             );
             CREATE TABLE IF NOT EXISTS conversation_events (
                 id         INTEGER PRIMARY KEY AUTOINCREMENT,
                 role       TEXT    NOT NULL,
                 kind       TEXT    NOT NULL,
                 summary    TEXT    NOT NULL,
                 created_at INTEGER NOT NULL
             );
             CREATE TABLE IF NOT EXISTS cost_snapshots (
                 month_yyyymm      INTEGER PRIMARY KEY,
                 total_nanodollars INTEGER NOT NULL
             );",
        )
        .map_err(|_| RecorderError::Open)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Takes the connection lock, converting a poisoned lock into a fixed error
    /// rather than propagating the panic (never crash the whole Tauri process
    /// because one op panicked while holding the lock).
    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Connection>, RecorderError> {
        self.conn.lock().map_err(|_| RecorderError::Db)
    }
}

impl RecorderAdapter for SqliteAdapter {
    fn save_note(&self, text: &str) -> Result<i64, RecorderError> {
        let conn = self.lock()?;
        // Parameterized: `text` is bound, never interpolated -> no SQL injection.
        conn.execute(
            "INSERT INTO notes (text, created_at) VALUES (?1, ?2)",
            params![text, now_ms()],
        )
        .map_err(|_| RecorderError::Db)?;
        Ok(conn.last_insert_rowid())
    }

    fn list_recent_notes(&self, limit: u32) -> Result<Vec<Note>, RecorderError> {
        let conn = self.lock()?;
        let mut stmt = conn
            .prepare("SELECT id, text, created_at FROM notes ORDER BY id DESC LIMIT ?1")
            .map_err(|_| RecorderError::Db)?;
        let rows = stmt
            .query_map(params![limit as i64], |r| {
                Ok(Note {
                    id: r.get(0)?,
                    text: r.get(1)?,
                    created_at: r.get(2)?,
                })
            })
            .map_err(|_| RecorderError::Db)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(|_| RecorderError::Db)?);
        }
        Ok(out)
    }

    fn log_conversation_event(
        &self,
        role: &str,
        kind: &str,
        summary: &str,
    ) -> Result<i64, RecorderError> {
        let conn = self.lock()?;
        conn.execute(
            "INSERT INTO conversation_events (role, kind, summary, created_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![role, kind, summary, now_ms()],
        )
        .map_err(|_| RecorderError::Db)?;
        Ok(conn.last_insert_rowid())
    }

    fn list_recent_events(&self, limit: u32) -> Result<Vec<ConversationEvent>, RecorderError> {
        let conn = self.lock()?;
        let mut stmt = conn
            .prepare(
                "SELECT id, role, kind, summary, created_at
                 FROM conversation_events ORDER BY id DESC LIMIT ?1",
            )
            .map_err(|_| RecorderError::Db)?;
        let rows = stmt
            .query_map(params![limit as i64], |r| {
                Ok(ConversationEvent {
                    id: r.get(0)?,
                    role: r.get(1)?,
                    kind: r.get(2)?,
                    summary: r.get(3)?,
                    created_at: r.get(4)?,
                })
            })
            .map_err(|_| RecorderError::Db)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(|_| RecorderError::Db)?);
        }
        Ok(out)
    }

    fn add_month_cost(
        &self,
        month_yyyymm: u32,
        delta_nanodollars: u64,
    ) -> Result<u64, RecorderError> {
        let conn = self.lock()?;
        // Additive ledger (koe-ixt): the monthly cost is the SUM of every session's
        // per-`response.done` charge, so we ADD this delta to the stored total
        // rather than overwriting it. This sums two sessions' spend that overlap
        // during a stop->start handover (an older read loop draining late usage)
        // instead of losing one side to a max(); it is order-independent so a late /
        // out-of-order write never rolls the total back (mechanism 5); and the
        // returned total lets the caller gate on the authoritative cross-session sum
        // (mechanism 4).
        //
        // The add is done in RUST with `saturating_add`, NOT SQLite's `+`: SQLite
        // promotes an i64 overflow to a REAL float, which would break the
        // integer-only, NaN/Inf-free budget invariant; saturating_add clamps at
        // u64::MAX (fail-closed = over budget). SQLite INTEGER is i64, so the u64 is
        // stored by *bit* reinterpretation (`as i64`) and read back with `as u64`,
        // round-tripping all 64 bits exactly (a saturated u64::MAX total survives a
        // restart as i64 -1 instead of clamping at i64::MAX). The whole
        // read-modify-write runs while THIS adapter's single connection Mutex is
        // held (every RecorderAdapter call takes it), so two session loops' adds
        // serialize and cannot interleave — atomic without a separate SQL
        // transaction, and no add is lost to a lost-update race.
        let current: Option<i64> = conn
            .query_row(
                "SELECT total_nanodollars FROM cost_snapshots WHERE month_yyyymm = ?1",
                params![month_yyyymm],
                |r| r.get(0),
            )
            .optional()
            .map_err(|_| RecorderError::Db)?;
        let new_total = current
            .map(|n| n as u64)
            .unwrap_or(0)
            .saturating_add(delta_nanodollars);
        conn.execute(
            "INSERT INTO cost_snapshots (month_yyyymm, total_nanodollars) VALUES (?1, ?2)
             ON CONFLICT(month_yyyymm) DO UPDATE SET total_nanodollars = excluded.total_nanodollars",
            params![month_yyyymm, new_total as i64],
        )
        .map_err(|_| RecorderError::Db)?;
        Ok(new_total)
    }

    fn load_cost_snapshot(&self, month_yyyymm: u32) -> Result<Option<u64>, RecorderError> {
        let conn = self.lock()?;
        let stored: Option<i64> = conn
            .query_row(
                "SELECT total_nanodollars FROM cost_snapshots WHERE month_yyyymm = ?1",
                params![month_yyyymm],
                |r| r.get(0),
            )
            .optional()
            .map_err(|_| RecorderError::Db)?;
        Ok(stored.map(|n| n as u64))
    }

    fn health_check(&self) -> Result<(), RecorderError> {
        let conn = self.lock()?;
        // Read-only: confirm the notes table is queryable. `.optional()` turns
        // the empty-table "no rows" into Ok(None) (still healthy). Crucially we
        // do NOT write a probe row (that would pollute list_recent_notes).
        conn.query_row("SELECT 1 FROM notes LIMIT 1", [], |_| Ok(()))
            .optional()
            .map(|_| ())
            .map_err(|_| RecorderError::Db)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    fn mem() -> SqliteAdapter {
        SqliteAdapter::open_in_memory().expect("open in-memory")
    }

    // ---- notes -------------------------------------------------------------

    #[test]
    fn save_then_list_round_trips() {
        let a = mem();
        let id = a.save_note("hello").expect("save");
        assert!(id > 0);
        let notes = a.list_recent_notes(10).expect("list");
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].text, "hello");
        assert_eq!(notes[0].id, id);
    }

    #[test]
    fn list_recent_notes_orders_newest_first() {
        let a = mem();
        a.save_note("first").unwrap();
        a.save_note("second").unwrap();
        a.save_note("third").unwrap();
        let notes = a.list_recent_notes(10).unwrap();
        let texts: Vec<_> = notes.iter().map(|n| n.text.as_str()).collect();
        assert_eq!(texts, ["third", "second", "first"]);
    }

    #[test]
    fn list_recent_notes_respects_limit() {
        let a = mem();
        for i in 0..5 {
            a.save_note(&format!("n{i}")).unwrap();
        }
        assert_eq!(a.list_recent_notes(2).unwrap().len(), 2);
    }

    #[test]
    fn list_recent_notes_limit_zero_returns_empty() {
        let a = mem();
        a.save_note("x").unwrap();
        assert!(a.list_recent_notes(0).unwrap().is_empty());
    }

    #[test]
    fn note_text_with_sql_metacharacters_is_stored_literally() {
        // Parameterized binding must treat an injection-looking string as data.
        let a = mem();
        let evil = "'); DROP TABLE notes;--";
        a.save_note(evil).unwrap();
        let notes = a.list_recent_notes(10).unwrap();
        // Table still exists (no drop) and the text round-trips verbatim.
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].text, evil);
    }

    #[test]
    fn note_text_with_interior_nul_round_trips() {
        // Control chars in fixtures are written as escape sequences, never
        // literal bytes (git.md: a literal NUL flips git to binary mode and
        // corrupts the diff).
        let a = mem();
        let with_nul = "before\u{0000}after";
        a.save_note(with_nul).unwrap();
        let notes = a.list_recent_notes(1).unwrap();
        // rusqlite binds &str by byte length, so an interior NUL is preserved
        // (SQLite TEXT is not C-string terminated here). Locks the behaviour so
        // a regression to NUL-truncation would be caught.
        assert_eq!(notes[0].text, with_nul);
    }

    #[test]
    fn persists_across_reopen() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("koe.db");
        {
            let a = SqliteAdapter::open(&path).expect("open 1");
            a.save_note("durable").unwrap();
        }
        // Fresh adapter on the same file: the note must still be there.
        let a = SqliteAdapter::open(&path).expect("open 2");
        let notes = a.list_recent_notes(10).unwrap();
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].text, "durable");
    }

    // ---- conversation events ----------------------------------------------

    #[test]
    fn conversation_event_round_trips() {
        let a = mem();
        a.log_conversation_event("assistant", "speech", "summarised reply")
            .unwrap();
        let events = a.list_recent_events(10).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].role, "assistant");
        assert_eq!(events[0].kind, "speech");
        assert_eq!(events[0].summary, "summarised reply");
    }

    #[test]
    fn list_recent_events_limit_zero_returns_empty() {
        let a = mem();
        a.log_conversation_event("user", "speech", "hi").unwrap();
        assert!(a.list_recent_events(0).unwrap().is_empty());
    }

    #[test]
    fn list_recent_events_orders_newest_first() {
        let a = mem();
        a.log_conversation_event("user", "speech", "first").unwrap();
        a.log_conversation_event("assistant", "speech", "second")
            .unwrap();
        a.log_conversation_event("user", "speech", "third").unwrap();
        let events = a.list_recent_events(10).unwrap();
        let summaries: Vec<_> = events.iter().map(|e| e.summary.as_str()).collect();
        assert_eq!(summaries, ["third", "second", "first"]);
    }

    #[test]
    fn conversation_event_fields_with_sql_metacharacters_are_literal() {
        let a = mem();
        let evil = "'); DROP TABLE conversation_events;--";
        a.log_conversation_event("user", "speech", evil).unwrap();
        let events = a.list_recent_events(10).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].summary, evil);
    }

    // ---- cost snapshots ----------------------------------------------------

    #[test]
    fn cost_snapshot_absent_is_none() {
        let a = mem();
        assert_eq!(a.load_cost_snapshot(202605).unwrap(), None);
    }

    #[test]
    fn add_month_cost_accumulates_and_returns_running_total() {
        // Additive ledger: each add sums onto the month's running total and returns
        // the new total. The first add of a fresh month starts from 0.
        let a = mem();
        assert_eq!(a.add_month_cost(202605, 1_000).unwrap(), 1_000);
        assert_eq!(a.add_month_cost(202605, 1_500).unwrap(), 2_500);
        assert_eq!(a.load_cost_snapshot(202605).unwrap(), Some(2_500));
    }

    #[test]
    fn add_month_cost_never_decreases_and_is_order_independent() {
        // koe-ixt mechanism 5 (undercount / rollback). An additive ledger only ever
        // grows within a month, so a LATE / out-of-order write (e.g. a stop->start
        // handover where an older session's late `response.done` add lands after a
        // newer session already added) can never roll the total backwards: it just
        // sums in. The returned total is the authoritative cross-session sum the
        // caller gates on.
        let a = mem();
        assert_eq!(a.add_month_cost(202605, 2_500).unwrap(), 2_500);
        // A smaller, "late" add increases the total (it does NOT overwrite/roll back).
        assert_eq!(
            a.add_month_cost(202605, 1_000).unwrap(),
            3_500,
            "a late add must sum in, never roll the total back"
        );
        assert_eq!(a.load_cost_snapshot(202605).unwrap(), Some(3_500));
    }

    #[test]
    fn add_month_cost_saturates_at_u64_max_without_real_promotion() {
        // The add is done in Rust with saturating_add (NOT SQLite's `+`, which
        // promotes an i64 overflow to a REAL float and breaks the integer-only,
        // NaN/Inf-free budget invariant). Adding onto a near-u64::MAX total clamps
        // at u64::MAX (fail-closed = over budget), and u64::MAX round-trips exactly
        // through the i64 bit-cast (it would be i64 -1, never i64::MAX).
        let a = mem();
        assert_eq!(a.add_month_cost(202605, u64::MAX - 10).unwrap(), u64::MAX - 10);
        assert_eq!(
            a.add_month_cost(202605, 1_000).unwrap(),
            u64::MAX,
            "an add that would overflow must saturate at u64::MAX, not wrap or go REAL"
        );
        assert_eq!(a.load_cost_snapshot(202605).unwrap(), Some(u64::MAX));
        // A further add stays clamped.
        assert_eq!(a.add_month_cost(202605, 5).unwrap(), u64::MAX);
    }

    #[test]
    fn add_month_cost_months_are_independent() {
        let a = mem();
        a.add_month_cost(202605, 100).unwrap();
        a.add_month_cost(202606, 200).unwrap();
        a.add_month_cost(202605, 50).unwrap();
        assert_eq!(a.load_cost_snapshot(202605).unwrap(), Some(150));
        assert_eq!(a.load_cost_snapshot(202606).unwrap(), Some(200));
    }

    #[test]
    fn add_month_cost_u64_max_round_trips() {
        // u64::MAX exceeds i64::MAX; the bit-cast store/load must preserve it
        // exactly (a saturated budget total must not corrupt on restart).
        let a = mem();
        a.add_month_cost(202605, u64::MAX).unwrap();
        assert_eq!(a.load_cost_snapshot(202605).unwrap(), Some(u64::MAX));
    }

    #[test]
    fn cost_ledger_persists_across_reopen() {
        // File-backed: the accumulated total (here a saturated u64::MAX) must
        // survive a restart exactly (persistence + the u64<->i64 bit round-trip).
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("koe.db");
        {
            let a = SqliteAdapter::open(&path).expect("open 1");
            a.add_month_cost(202605, u64::MAX).unwrap();
        }
        let a = SqliteAdapter::open(&path).expect("open 2");
        assert_eq!(a.load_cost_snapshot(202605).unwrap(), Some(u64::MAX));
    }

    // ---- health check ------------------------------------------------------

    #[test]
    fn health_check_ok_on_empty_db() {
        let a = mem();
        a.health_check().expect("healthy");
    }

    #[test]
    fn health_check_does_not_pollute_notes() {
        // A read-only probe must NOT create a row that list_recent_notes returns.
        let a = mem();
        a.health_check().unwrap();
        a.health_check().unwrap();
        assert!(a.list_recent_notes(10).unwrap().is_empty());
    }

    // ---- concurrency -------------------------------------------------------

    #[test]
    fn concurrent_writes_serialize_without_loss() {
        // 8 threads x 10 inserts = 80 rows; the Mutex serializes access and no
        // op panics, so every insert succeeds (a poisoned-lock failure would
        // surface as an explicit Err below, not a silent miscount).
        let a = Arc::new(mem());
        let mut handles = Vec::new();
        for t in 0..8 {
            let a = Arc::clone(&a);
            handles.push(thread::spawn(move || {
                for i in 0..10 {
                    a.save_note(&format!("t{t}-{i}")).expect("concurrent save");
                }
            }));
        }
        for h in handles {
            h.join().expect("thread join");
        }
        assert_eq!(a.list_recent_notes(1000).unwrap().len(), 80);
    }

    #[test]
    fn adapter_is_send_and_sync() {
        // Locks the invariant that SqliteAdapter (Mutex<Connection>) — and thus
        // ManagedRecorder(Arc<dyn RecorderAdapter>) — is shareable across the
        // async tool tasks Tauri spawns.
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<SqliteAdapter>();
    }

    // ---- structural: lib.rs wiring (locks the managed-state seam) ----------

    /// lib.rs with `//` comment lines stripped, so a doc comment mentioning a
    /// symbol does not satisfy the assertions below (mirrors the helper in
    /// secret_store.rs). Path is relative to THIS file: storage/sqlite.rs ->
    /// ../lib.rs.
    fn lib_rs_code_only() -> String {
        include_str!("../lib.rs")
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn lib_rs_wires_managed_recorder() {
        let code = lib_rs_code_only();
        assert!(
            code.contains("mod storage"),
            "lib.rs must declare mod storage"
        );
        assert!(
            code.contains("SqliteAdapter::open"),
            "lib.rs setup must open the SQLite recorder"
        );
        // Require the actual manage() call site, not just the `use` import: a
        // dropped registration would still satisfy a bare-symbol check.
        assert!(
            code.contains("app.manage(ManagedRecorder"),
            "lib.rs must register the recorder as Tauri managed state"
        );
    }

    #[test]
    fn lib_rs_does_not_register_sql_plugin() {
        // The recorder owns SQLite entirely in Rust; registering tauri-plugin-sql
        // would open a WebView SQL surface. Analogue of secret_store's
        // stronghold_plugin_is_not_registered guard.
        let code = lib_rs_code_only();
        assert!(
            !code.contains("tauri_plugin_sql") && !code.contains("tauri-plugin-sql"),
            "tauri-plugin-sql must not be registered (no WebView SQL surface)"
        );
    }
}
