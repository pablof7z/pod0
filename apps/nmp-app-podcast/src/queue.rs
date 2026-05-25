//! Actor-thread helpers for the `podcast.queue` / `PlayerAction`
//! queue ops (enqueue / dequeue / clear).
//!
//! Lives next to [`crate::categorization`] so the hot
//! [`crate::host_op_handler::PodcastHostOpHandler`] file stays under
//! the 500-line cap. `play_next` is intentionally not here because it
//! re-enters `handle_play`, which is bound to the handler struct.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crate::player::PlayerActor;
use crate::store::PodcastStore;

pub(crate) fn handle_enqueue(
    store: &Arc<Mutex<PodcastStore>>,
    player_actor: &Arc<Mutex<PlayerActor>>,
    rev: &AtomicU64,
    episode_id: String,
) -> serde_json::Value {
    let exists = match store.lock() {
        Ok(s) => s.episode_playback_info(&episode_id).is_some(),
        Err(_) => return serde_json::json!({"ok": false, "error": "store poisoned"}),
    };
    if !exists {
        return serde_json::json!({
            "ok": false,
            "error": format!("episode not found: {episode_id}")
        });
    }
    match player_actor.lock() {
        Ok(mut a) => {
            a.enqueue(&episode_id);
            rev.fetch_add(1, Ordering::Relaxed);
            serde_json::json!({"ok": true})
        }
        Err(_) => serde_json::json!({"ok": false, "error": "player_actor poisoned"}),
    }
}

pub(crate) fn handle_dequeue(
    player_actor: &Arc<Mutex<PlayerActor>>,
    rev: &AtomicU64,
    episode_id: String,
) -> serde_json::Value {
    match player_actor.lock() {
        Ok(mut a) => {
            a.dequeue(&episode_id);
            rev.fetch_add(1, Ordering::Relaxed);
            serde_json::json!({"ok": true})
        }
        Err(_) => serde_json::json!({"ok": false, "error": "player_actor poisoned"}),
    }
}

pub(crate) fn handle_clear_queue(
    player_actor: &Arc<Mutex<PlayerActor>>,
    rev: &AtomicU64,
) -> serde_json::Value {
    match player_actor.lock() {
        Ok(mut a) => {
            a.clear_queue();
            rev.fetch_add(1, Ordering::Relaxed);
            serde_json::json!({"ok": true})
        }
        Err(_) => serde_json::json!({"ok": false, "error": "player_actor poisoned"}),
    }
}
