//! Local persistence for Rhanis (rhanis-nnk).
//!
//! [`adapter::RecorderAdapter`] is the swappable backend contract; [`sqlite`]
//! is the default M1 implementation (Rust-owned SQLite via `rusqlite`, no
//! WebView SQL surface). See `adapter` for the security posture.

// Foundation module: every adapter method/type is exercised by the in-module
// tests but has no *production* caller yet — the consumers are the write_note
// tool (rhanis-s7i), session_manager (rhanis-e3m), and the settings health indicator
// (rhanis-200), none of which are merged. Allow dead_code module-wide until the
// first consumer lands, then drop this so any genuinely-unused item resurfaces.
#![allow(dead_code)]

pub mod adapter;
pub mod sqlite;
