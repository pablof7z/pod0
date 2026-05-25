//! Queue-action handler for `podcast.queue.*` host ops.
//!
//! Extracted into a sibling module so [`crate::host_op_handler`] stays under
//! the 500-line ceiling. The function operates over the same shared state
//! (`PlaybackQueue` + `rev`) the `PodcastHostOpHandler` carries, but does
//! not need the `NmpApp` pointer or any capability dispatcher — queue
//! mutations are pure in-memory writes.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crate::ffi::actions::queue_module::QueueAction;
use crate::queue::PlaybackQueue;

/// Apply a [`QueueAction`] to the shared queue and bump `rev` so the next
/// snapshot poll surfaces the change.
///
/// Returns the canonical `{"ok": true}` envelope on success; a typed error
/// envelope when the queue mutex is poisoned (D6).
pub(crate) fn handle_queue_action(
    queue: &Arc<Mutex<PlaybackQueue>>,
    rev: &Arc<AtomicU64>,
    action: QueueAction,
) -> serde_json::Value {
    let mut q = match queue.lock() {
        Ok(q) => q,
        Err(_) => return serde_json::json!({"ok": false, "error": "queue poisoned"}),
    };
    match action {
        QueueAction::AddNext { episode_id } => q.add_to_front(&episode_id),
        QueueAction::AddLast { episode_id } => q.add_to_end(&episode_id),
        QueueAction::Remove { episode_id } => q.remove(&episode_id),
        QueueAction::Clear => q.clear(),
    }
    drop(q);
    rev.fetch_add(1, Ordering::Relaxed);
    serde_json::json!({"ok": true})
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh() -> (Arc<Mutex<PlaybackQueue>>, Arc<AtomicU64>) {
        (
            Arc::new(Mutex::new(PlaybackQueue::new())),
            Arc::new(AtomicU64::new(0)),
        )
    }

    #[test]
    fn add_next_pushes_front_and_bumps_rev() {
        let (q, rev) = fresh();
        let result = handle_queue_action(
            &q,
            &rev,
            QueueAction::AddNext { episode_id: "ep-1".into() },
        );
        assert_eq!(result, serde_json::json!({"ok": true}));
        assert_eq!(q.lock().unwrap().items(), &["ep-1".to_owned()]);
        assert_eq!(rev.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn add_last_pushes_back_and_bumps_rev() {
        let (q, rev) = fresh();
        handle_queue_action(
            &q,
            &rev,
            QueueAction::AddLast { episode_id: "ep-1".into() },
        );
        handle_queue_action(
            &q,
            &rev,
            QueueAction::AddLast { episode_id: "ep-2".into() },
        );
        assert_eq!(
            q.lock().unwrap().items(),
            &["ep-1".to_owned(), "ep-2".to_owned()]
        );
        assert_eq!(rev.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn remove_drops_episode_and_bumps_rev() {
        let (q, rev) = fresh();
        handle_queue_action(
            &q,
            &rev,
            QueueAction::AddLast { episode_id: "ep-1".into() },
        );
        handle_queue_action(
            &q,
            &rev,
            QueueAction::AddLast { episode_id: "ep-2".into() },
        );
        let pre_rev = rev.load(Ordering::Relaxed);
        let result = handle_queue_action(
            &q,
            &rev,
            QueueAction::Remove { episode_id: "ep-1".into() },
        );
        assert_eq!(result, serde_json::json!({"ok": true}));
        assert_eq!(q.lock().unwrap().items(), &["ep-2".to_owned()]);
        assert_eq!(rev.load(Ordering::Relaxed), pre_rev + 1);
    }

    #[test]
    fn clear_empties_queue_and_bumps_rev() {
        let (q, rev) = fresh();
        handle_queue_action(
            &q,
            &rev,
            QueueAction::AddLast { episode_id: "ep-1".into() },
        );
        handle_queue_action(
            &q,
            &rev,
            QueueAction::AddLast { episode_id: "ep-2".into() },
        );
        let pre_rev = rev.load(Ordering::Relaxed);
        let result = handle_queue_action(&q, &rev, QueueAction::Clear);
        assert_eq!(result, serde_json::json!({"ok": true}));
        assert!(q.lock().unwrap().items().is_empty());
        assert_eq!(rev.load(Ordering::Relaxed), pre_rev + 1);
    }
}
