//! Actor-thread handlers for the two ad-skip actions:
//!
//! * `podcast.player.set_ad_segments` — persist the segments and (if
//!   the episode is the one currently loaded) push them into the
//!   active `PlayerActor` so auto-skip can fire immediately.
//! * `podcast.settings.set_auto_skip_ads` — mirror the toggle into
//!   `PodcastStore` (persistent) and `PlayerActor` (live).
//!
//! Extracted from `host_op_handler.rs` because that file is already at
//! the 500-line hard cap. Free functions take `Arc<Mutex<...>>` so the
//! caller can release locks before / between calls per the lock
//! discipline noted in `host_op_handler.rs` (no host-op handler ever
//! holds a store/actor lock across a `dispatch_capability` call).

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use serde_json::Value;

use crate::player::{AdSegment, PlayerActor};
use crate::store::PodcastStore;

/// Apply a `podcast.player.set_ad_segments` action: write to the store
/// and, when the episode is the one currently loaded, refresh the
/// active actor's segment list.
pub(crate) fn handle_set_ad_segments(
    store: &Arc<Mutex<PodcastStore>>,
    player_actor: &Arc<Mutex<PlayerActor>>,
    rev: &Arc<AtomicU64>,
    episode_id: String,
    segments: Vec<AdSegment>,
) -> Value {
    {
        match store.lock() {
            Ok(mut s) => s.set_ad_segments_for(episode_id.clone(), segments.clone()),
            Err(_) => return serde_json::json!({"ok": false, "error": "store poisoned"}),
        }
    }
    // Refresh the live actor only when the action targets the currently
    // loaded episode — otherwise the next `play` will pick the new
    // segments up via `set_ad_segments` in `handle_play`'s extension.
    {
        if let Ok(mut actor) = player_actor.lock() {
            if actor.state().episode_id.as_deref() == Some(episode_id.as_str()) {
                actor.set_ad_segments(segments);
            }
        }
    }
    rev.fetch_add(1, Ordering::Relaxed);
    serde_json::json!({"ok": true})
}

/// Apply a `podcast.settings.set_auto_skip_ads` action: mirror the
/// boolean into both the persistent store and the active actor so the
/// next `Playing` tick sees the new value.
pub(crate) fn handle_set_auto_skip_ads(
    store: &Arc<Mutex<PodcastStore>>,
    player_actor: &Arc<Mutex<PlayerActor>>,
    rev: &Arc<AtomicU64>,
    enabled: bool,
) -> Value {
    {
        match store.lock() {
            Ok(mut s) => s.set_auto_skip_ads_enabled(enabled),
            Err(_) => return serde_json::json!({"ok": false, "error": "store poisoned"}),
        }
    }
    {
        if let Ok(mut actor) = player_actor.lock() {
            actor.set_auto_skip_ads(enabled);
        }
    }
    rev.fetch_add(1, Ordering::Relaxed);
    serde_json::json!({"ok": true})
}

/// Helper used by `host_op_handler::handle_play` to push the stored
/// ad segments + global toggle into a freshly-staged actor before
/// `AudioCommand::Load` is dispatched. Pure read on the store side.
pub(crate) fn hydrate_actor_for_play(
    store: &Arc<Mutex<PodcastStore>>,
    player_actor: &Arc<Mutex<PlayerActor>>,
    episode_id: &str,
) {
    let (segments, enabled) = match store.lock() {
        Ok(s) => (s.ad_segments_for(episode_id).to_vec(), s.auto_skip_ads_enabled()),
        Err(_) => return,
    };
    if let Ok(mut actor) = player_actor.lock() {
        actor.set_ad_segments(segments);
        actor.set_auto_skip_ads(enabled);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use podcast_core::AdKind;

    fn seg(start: f64, end: f64) -> AdSegment {
        AdSegment::new(start, end, AdKind::Midroll)
    }

    fn rev() -> Arc<AtomicU64> {
        Arc::new(AtomicU64::new(1))
    }

    #[test]
    fn set_auto_skip_ads_propagates_to_store_and_actor() {
        let store = Arc::new(Mutex::new(PodcastStore::new()));
        let actor = Arc::new(Mutex::new(PlayerActor::new()));
        let r = rev();

        let res = handle_set_auto_skip_ads(&store, &actor, &r, true);
        assert_eq!(res["ok"], true);
        assert!(store.lock().unwrap().auto_skip_ads_enabled());
        assert!(actor.lock().unwrap().auto_skip_ads());
        assert!(r.load(Ordering::Relaxed) > 1);
    }

    #[test]
    fn set_ad_segments_writes_to_store() {
        let store = Arc::new(Mutex::new(PodcastStore::new()));
        let actor = Arc::new(Mutex::new(PlayerActor::new()));
        let r = rev();

        let segs = vec![seg(30.0, 60.0)];
        let res = handle_set_ad_segments(&store, &actor, &r, "ep-1".into(), segs.clone());
        assert_eq!(res["ok"], true);
        let stored = store.lock().unwrap().ad_segments_for("ep-1").to_vec();
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].start_secs, 30.0);
        // Actor untouched — the episode isn't loaded.
        assert!(actor.lock().unwrap().ad_segments().is_empty());
    }

    #[test]
    fn set_ad_segments_refreshes_active_actor_when_episode_matches() {
        let store = Arc::new(Mutex::new(PodcastStore::new()));
        let actor = Arc::new(Mutex::new(PlayerActor::new()));
        let r = rev();
        actor
            .lock()
            .unwrap()
            .stage_load("ep-1", Some("pod-1".into()), "https://ex.com", 0.0);

        let segs = vec![seg(30.0, 60.0)];
        let _ = handle_set_ad_segments(&store, &actor, &r, "ep-1".into(), segs.clone());
        assert_eq!(actor.lock().unwrap().ad_segments().len(), 1);
    }

    #[test]
    fn hydrate_actor_for_play_copies_store_into_actor() {
        let store = Arc::new(Mutex::new(PodcastStore::new()));
        let actor = Arc::new(Mutex::new(PlayerActor::new()));
        {
            let mut s = store.lock().unwrap();
            s.set_ad_segments_for("ep-1", vec![seg(30.0, 60.0)]);
            s.set_auto_skip_ads_enabled(true);
        }

        hydrate_actor_for_play(&store, &actor, "ep-1");
        let a = actor.lock().unwrap();
        assert_eq!(a.ad_segments().len(), 1);
        assert!(a.auto_skip_ads());
    }
}
