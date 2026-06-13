//! Shared global activity-event sequence (rhanis-1vi).
//!
//! A single process-wide monotonic counter so that `ToolEvent.sequence`
//! (emitted by the tool_dispatcher, rhanis-2gy) and `ApprovalRequest.sequence`
//! (emitted by the approval gate, this PR) share ONE ordering space — exactly
//! as `src/features/activity/types.ts` specifies ("Globally monotonic counter
//! ... shared space with ToolEvent.sequence").
//!
//! It lives in its own module so the tool_dispatcher can reuse it via
//! `tauri::State<'_, ManagedSequenceCounter>` WITHOUT depending on
//! `approval_gate` (git.md: extract the shared resource + lock the import path
//! so two modules never grow divergent counters).

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Process-wide monotonic sequence source for activity events.
#[derive(Debug, Default)]
pub struct SequenceCounter {
    next: AtomicU64,
}

impl SequenceCounter {
    pub fn new() -> Self {
        Self {
            next: AtomicU64::new(0),
        }
    }

    /// Returns the next sequence value (the first call returns 0).
    ///
    /// `Relaxed` is sufficient: each `fetch_add` is itself atomic and we only
    /// need uniqueness + monotonicity for UI display ordering, not ordering
    /// relative to other memory operations. The counter wraps after 2^64 emits
    /// — unreachable in practice, and a wrap only reuses display-order numbers,
    /// never panics.
    pub fn next(&self) -> u64 {
        self.next.fetch_add(1, Ordering::Relaxed)
    }
}

/// Tauri managed-state wrapper. The approval gate (rhanis-1vi) and the
/// tool_dispatcher (rhanis-2gy) hold clones of the SAME `Arc<SequenceCounter>`;
/// rhanis-2gy obtains it through `tauri::State<'_, ManagedSequenceCounter>` rather
/// than importing the gate.
///
/// `lib.rs` registers this now so the counter is shared from day one, but its
/// only *reader* is the not-yet-merged tool_dispatcher (rhanis-2gy) — hence
/// `#[allow(dead_code)]` on the field (interface-first, like
/// `secret_store::SecretStore::get_api_key`), not skeleton.
pub struct ManagedSequenceCounter(#[allow(dead_code)] pub Arc<SequenceCounter>);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sequence_is_monotonic_from_zero() {
        let c = SequenceCounter::new();
        assert_eq!(c.next(), 0);
        assert_eq!(c.next(), 1);
        assert_eq!(c.next(), 2);
    }

    #[test]
    fn shared_arc_is_one_sequence_space() {
        // The gate and the dispatcher must NOT each get a private counter; a
        // clone of the Arc must continue the same atomic.
        let a = Arc::new(SequenceCounter::new());
        let b = Arc::clone(&a);
        assert_eq!(a.next(), 0);
        assert_eq!(b.next(), 1);
        assert_eq!(a.next(), 2);
    }

    #[test]
    fn default_starts_at_zero() {
        assert_eq!(SequenceCounter::default().next(), 0);
    }

    #[test]
    fn counter_is_send_and_sync() {
        // Must be shareable across the async tool tasks + Tauri state.
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<SequenceCounter>();
        assert_send_sync::<ManagedSequenceCounter>();
    }
}
