//! Actor-thread handler for `"podcast.memory.*"` host operations.
//!
//! Sibling of [`crate::host_op_handler`]; lives in its own file so
//! `host_op_handler.rs` stays under the 500-line hard cap. The handler
//! owns no state of its own — it borrows the shared `Arc<Mutex<PodcastStore>>`
//! and `rev` counter from the existing `PodcastHostOpHandler`. The snapshot
//! reader and memory writer share the same lock discipline as every other
//! host op.
//!
//! Each handler returns a `serde_json::Value` envelope (`{"ok":true}` /
//! `{"ok":false,"error":...}`) matching the rest of the host-op responses.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use chrono::Utc;

use crate::ffi::actions::memory_module::MemoryAction;
use crate::store::PodcastStore;

/// Dispatch the typed [`MemoryAction`] to the shared `store`, bumping `rev`
/// when something actually changed so the next iOS snapshot frame picks it up.
///
/// `source` defaults to `"user"` when the wire payload omits it — matches
/// the [`MemoryAction::Remember.source`] doc-comment and keeps Settings'
/// hand-rolled dispatch terse.
pub fn handle(
    action: MemoryAction,
    store: &Arc<Mutex<PodcastStore>>,
    rev: &Arc<AtomicU64>,
) -> serde_json::Value {
    match action {
        MemoryAction::Remember { key, value, source } => {
            if key.trim().is_empty() {
                return serde_json::json!({"ok": false, "error": "empty key"});
            }
            let resolved_source = source.unwrap_or_else(|| "user".to_owned());
            let now = Utc::now().timestamp();
            match store.lock() {
                Ok(mut s) => {
                    s.set_memory_fact(key, value, resolved_source, now);
                    rev.fetch_add(1, Ordering::Relaxed);
                    serde_json::json!({"ok": true})
                }
                Err(_) => serde_json::json!({"ok": false, "error": "store poisoned"}),
            }
        }
        MemoryAction::Forget { key } => match store.lock() {
            Ok(mut s) => {
                if s.remove_memory_fact(&key) {
                    rev.fetch_add(1, Ordering::Relaxed);
                }
                serde_json::json!({"ok": true})
            }
            Err(_) => serde_json::json!({"ok": false, "error": "store poisoned"}),
        },
        MemoryAction::ForgetAll => match store.lock() {
            Ok(mut s) => {
                if s.clear_memory() > 0 {
                    rev.fetch_add(1, Ordering::Relaxed);
                }
                serde_json::json!({"ok": true})
            }
            Err(_) => serde_json::json!({"ok": false, "error": "store poisoned"}),
        },
    }
}

#[cfg(test)]
#[path = "memory_handler_tests.rs"]
mod tests;
