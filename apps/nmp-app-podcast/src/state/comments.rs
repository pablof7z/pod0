//! Comments substate — Step 8 of the god-root consolidation.
//!
//! Owns the two slots that were previously mirrored between
//! `PodcastHandle` and `PodcastHostOpHandler`:
//!
//! * `cache` — `HashMap<episode_id, Vec<CommentSummary>>`.  **Session**
//!   durability (comments re-fetch on next `FetchComments` dispatch).
//! * `viewed_episode_id` — `Option<String>` tracking which episode's
//!   comments the user is currently viewing.  **Session** durability.
//!
//! ## Observer wiring
//!
//! `CommentsObserver` (in `crate::comments_handler`) writes `cache` off the
//! actor thread via a `.share()`'d `Arc<Mutex<_>>` obtained from
//! `state.comments.cache.share()` at registration time.  Lock topology is
//! unchanged: the observer takes the cache lock independently, never nested
//! with any other lock.
//!
//! ## File-length ceiling
//!
//! AGENTS.md hard limit is 500 lines.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::ffi::projections::CommentSummary;
use crate::state::slot::Session;
use crate::state::{Infra, Slot};
use crate::store::identity::IdentityStore;
use crate::store::PodcastStore;

/// Comments feature substate.
///
/// Constructed once in `PodcastAppState::new` and referenced via
/// `state.comments` on both seams.  All methods are `&self`.
pub struct CommentsState {
    /// NIP-22 (kind 1111) comment cache, keyed by episode_id string.
    /// Written by `handle_fetch_comments` / `handle_post_comment` on the
    /// actor thread, and by `CommentsObserver` off the actor thread.
    /// Session durability — comments re-fetch on the next FetchComments action.
    pub cache: Slot<HashMap<String, Vec<CommentSummary>>, Session>,
    /// Episode id whose comments the user is currently viewing.
    /// Set by `handle_fetch_comments`; the snapshot projects this episode's
    /// cache slice instead of the now-playing episode's.
    /// `None` until the first `FetchComments` dispatch.
    pub viewed_episode_id: Slot<Option<String>, Session>,
    /// Rev + signal + runtime (cloned from `PodcastAppState::infra`).
    pub(crate) infra: Infra,
    /// The canonical persisted library — used by `handle_fetch_comments`
    /// and `handle_post_comment` for episode anchor resolution.
    pub(crate) store: Arc<Mutex<PodcastStore>>,
    /// Identity store — read by `handle_post_comment` to get the active npub.
    pub(crate) identity: Arc<Mutex<IdentityStore>>,
}

impl CommentsState {
    /// Production constructor — called from `PodcastAppState::new`.
    pub fn new(
        infra: Infra,
        store: Arc<Mutex<PodcastStore>>,
        identity: Arc<Mutex<IdentityStore>>,
    ) -> Self {
        Self {
            cache: Slot::new(HashMap::new()),
            viewed_episode_id: Slot::new(None),
            infra,
            store,
            identity,
        }
    }

    /// Test constructor — no `NmpApp` needed.
    #[cfg(test)]
    pub fn for_test(
        store: Arc<Mutex<PodcastStore>>,
        identity: Arc<Mutex<IdentityStore>>,
    ) -> Self {
        Self::new(Infra::for_test(), store, identity)
    }

    // ── Snapshot projection ───────────────────────────────────────────────

    /// Project the cache slice for the episode the user is viewing (falling
    /// back to `now_playing_episode_id`).  Returns an empty vec when neither
    /// is set or no comments are cached.
    ///
    /// Called by `build_podcast_update` as
    /// `state.comments.project(now_playing_episode_id)`.
    pub fn project(&self, now_playing_episode_id: Option<&str>) -> Vec<CommentSummary> {
        let viewed = self
            .viewed_episode_id
            .lock()
            .ok()
            .and_then(|v| v.clone());
        self.cache
            .lock()
            .ok()
            .and_then(|cache| {
                viewed
                    .as_deref()
                    .or(now_playing_episode_id)
                    .and_then(|ep_id| cache.get(ep_id).cloned())
            })
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use crate::ffi::projections::CommentSummary;
    use crate::store::{identity::IdentityStore, PodcastStore};

    use super::*;

    #[test]
    fn project_empty_when_no_data() {
        let store = Arc::new(Mutex::new(PodcastStore::new()));
        let identity = Arc::new(Mutex::new(IdentityStore::new()));
        let state = CommentsState::for_test(store, identity);
        assert!(state.project(None).is_empty());
    }

    #[test]
    fn project_falls_back_to_now_playing() {
        let store = Arc::new(Mutex::new(PodcastStore::new()));
        let identity = Arc::new(Mutex::new(IdentityStore::new()));
        let state = CommentsState::for_test(store, identity);

        {
            let mut cache = state.cache.lock().unwrap();
            cache.insert(
                "ep-1".to_string(),
                vec![CommentSummary {
                    id: "c1".into(),
                    author_npub: "npub1".into(),
                    author_name: None,
                    content: "hello".into(),
                    created_at: 0,
                }],
            );
        }

        let comments = state.project(Some("ep-1"));
        assert_eq!(comments.len(), 1);
        assert_eq!(comments[0].id, "c1");
    }

    #[test]
    fn project_prefers_viewed_over_now_playing() {
        let store = Arc::new(Mutex::new(PodcastStore::new()));
        let identity = Arc::new(Mutex::new(IdentityStore::new()));
        let state = CommentsState::for_test(store, identity);

        {
            let mut cache = state.cache.lock().unwrap();
            cache.insert(
                "ep-viewed".to_string(),
                vec![CommentSummary {
                    id: "c-viewed".into(),
                    author_npub: "npub1".into(),
                    author_name: None,
                    content: "viewed comment".into(),
                    created_at: 0,
                }],
            );
            cache.insert(
                "ep-playing".to_string(),
                vec![CommentSummary {
                    id: "c-playing".into(),
                    author_npub: "npub2".into(),
                    author_name: None,
                    content: "playing comment".into(),
                    created_at: 0,
                }],
            );
        }
        {
            let mut viewed = state.viewed_episode_id.lock().unwrap();
            *viewed = Some("ep-viewed".to_string());
        }

        let comments = state.project(Some("ep-playing"));
        assert_eq!(comments.len(), 1);
        assert_eq!(comments[0].id, "c-viewed");
    }
}
