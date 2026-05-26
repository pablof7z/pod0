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
/// when something actually changed so the iOS snapshot poll picks it up on
/// the next 500ms tick.
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
mod tests {
    use super::*;

    fn fresh() -> (Arc<Mutex<PodcastStore>>, Arc<AtomicU64>) {
        (
            Arc::new(Mutex::new(PodcastStore::new())),
            Arc::new(AtomicU64::new(0)),
        )
    }

    #[test]
    fn remember_writes_through_to_store_and_bumps_rev() {
        let (store, rev) = fresh();
        let resp = handle(
            MemoryAction::Remember {
                key: "k".into(),
                value: "v".into(),
                source: None,
            },
            &store,
            &rev,
        );
        assert_eq!(resp["ok"], true);
        assert_eq!(rev.load(Ordering::Relaxed), 1);
        let facts = store.lock().unwrap().all_memory_facts();
        assert_eq!(facts.len(), 1);
        // Missing source defaults to "user".
        assert_eq!(facts[0].source, "user");
        assert_eq!(facts[0].key, "k");
    }

    #[test]
    fn remember_with_explicit_agent_source_is_recorded() {
        let (store, rev) = fresh();
        let resp = handle(
            MemoryAction::Remember {
                key: "k".into(),
                value: "v".into(),
                source: Some("agent".into()),
            },
            &store,
            &rev,
        );
        assert_eq!(resp["ok"], true);
        assert_eq!(
            store.lock().unwrap().all_memory_facts()[0].source,
            "agent"
        );
    }

    #[test]
    fn remember_rejects_empty_key() {
        let (store, rev) = fresh();
        let resp = handle(
            MemoryAction::Remember {
                key: "   ".into(),
                value: "v".into(),
                source: None,
            },
            &store,
            &rev,
        );
        assert_eq!(resp["ok"], false);
        // Rejected ⇒ no rev bump, no store write.
        assert_eq!(rev.load(Ordering::Relaxed), 0);
        assert!(store.lock().unwrap().all_memory_facts().is_empty());
    }

    #[test]
    fn forget_existing_key_bumps_rev() {
        let (store, rev) = fresh();
        store
            .lock()
            .unwrap()
            .set_memory_fact("k".into(), "v".into(), "user".into(), 1);
        let resp = handle(MemoryAction::Forget { key: "k".into() }, &store, &rev);
        assert_eq!(resp["ok"], true);
        assert_eq!(rev.load(Ordering::Relaxed), 1);
        assert!(store.lock().unwrap().all_memory_facts().is_empty());
    }

    #[test]
    fn forget_missing_key_is_ok_without_rev_bump() {
        let (store, rev) = fresh();
        let resp = handle(MemoryAction::Forget { key: "k".into() }, &store, &rev);
        assert_eq!(resp["ok"], true);
        // Nothing changed — no need to re-poll the snapshot.
        assert_eq!(rev.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn forget_all_clears_and_bumps_rev_once_when_non_empty() {
        let (store, rev) = fresh();
        store
            .lock()
            .unwrap()
            .set_memory_fact("a".into(), "1".into(), "user".into(), 1);
        store
            .lock()
            .unwrap()
            .set_memory_fact("b".into(), "2".into(), "user".into(), 2);
        let resp = handle(MemoryAction::ForgetAll, &store, &rev);
        assert_eq!(resp["ok"], true);
        // One bump for the whole wipe — not one per fact.
        assert_eq!(rev.load(Ordering::Relaxed), 1);
        assert!(store.lock().unwrap().all_memory_facts().is_empty());
    }

    #[test]
    fn forget_all_on_empty_store_is_noop_without_rev_bump() {
        let (store, rev) = fresh();
        let resp = handle(MemoryAction::ForgetAll, &store, &rev);
        assert_eq!(resp["ok"], true);
        assert_eq!(rev.load(Ordering::Relaxed), 0);
    }
}
