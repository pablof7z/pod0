use super::*;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::sync::atomic::AtomicU64 as SeqCounter;

use crate::store::PodcastStore;

fn fresh() -> (Arc<Mutex<PlaybackQueue>>, Arc<Mutex<PodcastStore>>, Arc<AtomicU64>) {
    (
        Arc::new(Mutex::new(PlaybackQueue::new())),
        // No data dir — persist is a silent no-op (D6), keeps tests hermetic.
        Arc::new(Mutex::new(PodcastStore::new())),
        Arc::new(AtomicU64::new(0)),
    )
}

/// Minimal RAII temp directory — avoids pulling in `tempfile` as a dev-dep.
struct TempDir {
    path: PathBuf,
}
impl TempDir {
    fn new() -> Self {
        static SEQ: SeqCounter = SeqCounter::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir()
            .join(format!("nmp-queue-test-{}-{}", std::process::id(), n));
        std::fs::create_dir_all(&path).expect("create temp dir");
        Self { path }
    }
}
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

#[test]
fn add_next_pushes_front_and_bumps_rev() {
    let (q, store, rev) = fresh();
    let result = handle_queue_action(
        &q,
        &store,
        &rev,
        QueueAction::AddNext { episode_id: "ep-1".into() },
    );
    assert_eq!(result, serde_json::json!({"ok": true}));
    assert_eq!(q.lock().unwrap().items(), &["ep-1".to_owned()]);
    assert_eq!(rev.load(Ordering::Relaxed), 1);
}
#[test]
fn add_last_pushes_back_and_bumps_rev() {
    let (q, store, rev) = fresh();
    handle_queue_action(
        &q,
        &store,
        &rev,
        QueueAction::AddLast { episode_id: "ep-1".into() },
    );
    handle_queue_action(
        &q,
        &store,
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
    let (q, store, rev) = fresh();
    handle_queue_action(
        &q,
        &store,
        &rev,
        QueueAction::AddLast { episode_id: "ep-1".into() },
    );
    handle_queue_action(
        &q,
        &store,
        &rev,
        QueueAction::AddLast { episode_id: "ep-2".into() },
    );
    let pre_rev = rev.load(Ordering::Relaxed);
    let result = handle_queue_action(
        &q,
        &store,
        &rev,
        QueueAction::Remove { episode_id: "ep-1".into() },
    );
    assert_eq!(result, serde_json::json!({"ok": true}));
    assert_eq!(q.lock().unwrap().items(), &["ep-2".to_owned()]);
    assert_eq!(rev.load(Ordering::Relaxed), pre_rev + 1);
}
#[test]
fn clear_empties_queue_and_bumps_rev() {
    let (q, store, rev) = fresh();
    handle_queue_action(
        &q,
        &store,
        &rev,
        QueueAction::AddLast { episode_id: "ep-1".into() },
    );
    handle_queue_action(
        &q,
        &store,
        &rev,
        QueueAction::AddLast { episode_id: "ep-2".into() },
    );
    let pre_rev = rev.load(Ordering::Relaxed);
    let result = handle_queue_action(&q, &store, &rev, QueueAction::Clear);
    assert_eq!(result, serde_json::json!({"ok": true}));
    assert!(q.lock().unwrap().items().is_empty());
    assert_eq!(rev.load(Ordering::Relaxed), pre_rev + 1);
}

#[test]
fn queue_persists_across_restart() {
    let tmp = TempDir::new();

    // first "launch": add two episodes
    let store1 = Arc::new(Mutex::new({
        let mut s = PodcastStore::new();
        s.set_data_dir(tmp.path.clone());
        s
    }));
    let q1 = Arc::new(Mutex::new(PlaybackQueue::new()));
    let rev1 = Arc::new(AtomicU64::new(0));
    handle_queue_action(
        &q1,
        &store1,
        &rev1,
        QueueAction::AddLast { episode_id: "ep-a".into() },
    );
    handle_queue_action(
        &q1,
        &store1,
        &rev1,
        QueueAction::AddLast { episode_id: "ep-b".into() },
    );
    assert_eq!(
        q1.lock().unwrap().items(),
        &["ep-a".to_owned(), "ep-b".to_owned()]
    );

    // second "launch": load from same data dir
    let mut store2 = PodcastStore::new();
    store2.set_data_dir(tmp.path.clone());
    let restored = store2.take_loaded_queue();
    assert_eq!(restored, vec!["ep-a".to_owned(), "ep-b".to_owned()]);
}

#[test]
fn clear_persists_empty_queue() {
    let tmp = TempDir::new();

    let store = Arc::new(Mutex::new({
        let mut s = PodcastStore::new();
        s.set_data_dir(tmp.path.clone());
        s
    }));
    let q = Arc::new(Mutex::new(PlaybackQueue::new()));
    let rev = Arc::new(AtomicU64::new(0));

    handle_queue_action(
        &q,
        &store,
        &rev,
        QueueAction::AddLast { episode_id: "ep-x".into() },
    );
    handle_queue_action(&q, &store, &rev, QueueAction::Clear);

    let mut reload = PodcastStore::new();
    reload.set_data_dir(tmp.path.clone());
    assert!(reload.take_loaded_queue().is_empty());
}
