//! Clips substate — Step 5a of the god-root consolidation.
//!
//! Owns the single slot that was previously mirrored between
//! `PodcastHandle` and `PodcastHostOpHandler`:
//!
//! * `clips` — user-saved audio clips, persisted to `clips.json` when a data
//!   directory is bound.
//!
//! `ClipHandler` (the existing struct in `crate::clip_handler`) already
//! encapsulates the action logic.  This substate composes it: the slot and
//! the `store` Arc live here; `ClipsState::handle` builds a short-lived
//! `ClipHandler` view and delegates.  The projection helper
//! `clip_handler::project_clips` is also invoked here so the snapshot path
//! reaches it through `state.clips.project(library)` instead of
//! `clip_handler::project_clips(&handle.clips, &library)`.
//!
//! ## File-length ceiling
//!
//! AGENTS.md hard limit is 500 lines.

use std::sync::{Arc, Mutex};

use crate::clip_handler::{ClipHandler, ClipRecord};
use crate::ffi::actions::clip_module::ClipAction;
use crate::ffi::projections::{ClipSummary, PodcastSummary};
use crate::state::slot::Persisted;
use crate::state::{Infra, Slot};
use crate::store::PodcastStore;

/// Clips feature substate.
///
/// Constructed once in `PodcastAppState::new` and referenced via
/// `state.clips` on both seams.  All methods are `&self`.
pub struct ClipsState {
    /// Rust-owned clip list, persisted to `clips.json`.
    pub clips: Slot<Vec<ClipRecord>, Persisted>,
    /// Rev + signal + runtime (cloned from `PodcastAppState::infra`).
    infra: Infra,
    /// The canonical persisted library — read by `ClipHandler` at create /
    /// auto-snip time to look up episode + podcast titles.
    store: Arc<Mutex<PodcastStore>>,
}

impl ClipsState {
    /// Production constructor — called from `PodcastAppState::new`.
    pub fn new(infra: Infra, store: Arc<Mutex<PodcastStore>>) -> Self {
        Self {
            clips: Slot::new(Vec::new()),
            infra,
            store,
        }
    }

    /// Test constructor — no `NmpApp` needed.
    #[cfg(test)]
    pub fn for_test(store: Arc<Mutex<PodcastStore>>) -> Self {
        Self::new(Infra::for_test(), store)
    }

    // ── Snapshot projection ───────────────────────────────────────────────

    /// Project the in-memory clip list into wire-format `ClipSummary` rows.
    ///
    /// `build_podcast_update` calls this instead of
    /// `clip_handler::project_clips(&handle.clips, &library)`.
    pub fn project(&self, library: &[PodcastSummary]) -> Vec<ClipSummary> {
        crate::clip_handler::project_clips(&self.clips.share(), library)
    }

    /// Return the current clip row for `id`, if present.
    pub fn clip(&self, id: &str) -> Option<ClipRecord> {
        let clips = self.clips.lock().ok()?;
        clips.iter().find(|rec| rec.id == id).cloned()
    }

    // ── Action handler ────────────────────────────────────────────────────

    /// Route a single `podcast.clip.*` action.
    ///
    /// Replaces the inline `ClipHandler::new(self.clips.clone(), …).handle(action)`
    /// construction in the router.
    pub fn handle(&self, action: ClipAction) -> serde_json::Value {
        // Build a short-lived `ClipHandler` view: passes `share()` so the
        // existing `ClipHandler` code retains its `Arc<Mutex<_>>` discipline.
        // The `infra.rev` Arc is shared — bump is still performed inside
        // `ClipHandler` via the raw `AtomicU64` (pre-`infra` style); we keep
        // that for now rather than rewiring all the private methods, but the
        // rev increment is the SAME Arc so the snapshot sees it.
        ClipHandler::new(
            self.clips.share(),
            self.store.clone(),
            self.infra.rev.clone(),
        )
        .handle(action)
    }

    /// Re-run kernel-owned autosnip refinement for clips that were captured
    /// before timed transcript entries arrived.
    pub fn refine_pending_for_episode(&self, episode_id: &str) -> Vec<ClipRecord> {
        ClipHandler::new(
            self.clips.share(),
            self.store.clone(),
            self.infra.rev.clone(),
        )
        .refine_pending_for_episode(episode_id)
    }

    /// Hydrate persisted clips from `<data_dir>/clips.json`.
    ///
    /// Returns true when a valid sidecar existed and was applied.
    pub fn set_data_dir(&self, dir: &std::path::Path) -> bool {
        let Some(restored) = crate::store::clip_records::load_clip_records(dir) else {
            return false;
        };
        let Ok(mut clips) = self.clips.lock() else {
            return false;
        };
        *clips = restored;
        true
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use podcast_core::{Episode, Podcast};
    use url::Url;

    use crate::ffi::actions::clip_module::ClipAction;
    use crate::store::PodcastStore;

    use super::*;

    fn make_store_with_episode() -> Arc<Mutex<PodcastStore>> {
        let mut store = PodcastStore::new();
        let podcast = Podcast::new("Test Show");
        let pid = podcast.id;
        let ep = Episode::new(
            pid,
            "https://example.com/feed.xml",
            "guid-1",
            "Episode One",
            Url::parse("https://example.com/1.mp3").unwrap(),
            chrono::Utc::now(),
        );
        store.subscribe(podcast, vec![ep.clone()]);
        Arc::new(Mutex::new(store))
    }

    fn episode_id_str(store: &Arc<Mutex<PodcastStore>>) -> String {
        let s = store.lock().unwrap();
        s.all_podcasts()
            .into_iter()
            .flat_map(|(_, eps)| eps.iter())
            .next()
            .unwrap()
            .id
            .0
            .to_string()
    }

    #[test]
    fn create_clip_bumps_rev() {
        let store = make_store_with_episode();
        let ep_id = episode_id_str(&store);
        let state = ClipsState::for_test(store);
        let rev0 = state.infra.rev();
        let out = state.handle(ClipAction::Create {
            episode_id: ep_id,
            start_secs: 10.0,
            end_secs: 40.0,
            title: Some("test clip".into()),
            source: None,
            transcript_text: None,
            client_clip_id: None,
        });
        assert_eq!(out["ok"], true, "create must succeed");
        assert!(out["clip_id"].is_string(), "must return clip_id");
        assert!(state.infra.rev() > rev0, "must bump rev");
        let clips = state.clips.lock().unwrap();
        assert_eq!(clips.len(), 1);
    }

    #[test]
    fn delete_clip_bumps_rev() {
        let store = make_store_with_episode();
        let ep_id = episode_id_str(&store);
        let state = ClipsState::for_test(store);

        let out = state.handle(ClipAction::Create {
            episode_id: ep_id,
            start_secs: 0.0,
            end_secs: 60.0,
            title: None,
            source: None,
            transcript_text: None,
            client_clip_id: None,
        });
        let clip_id = out["clip_id"].as_str().unwrap().to_owned();
        let rev1 = state.infra.rev();

        let out2 = state.handle(ClipAction::Delete {
            clip_id: clip_id.clone(),
        });
        assert_eq!(out2["ok"], true);
        assert!(state.infra.rev() > rev1, "delete must bump rev");
        assert!(state.clips.lock().unwrap().is_empty());
    }

    #[test]
    fn create_clip_episode_not_found_returns_error() {
        let state = ClipsState::for_test(Arc::new(Mutex::new(PodcastStore::new())));
        let out = state.handle(ClipAction::Create {
            episode_id: "nonexistent".into(),
            start_secs: 0.0,
            end_secs: 10.0,
            title: None,
            source: None,
            transcript_text: None,
            client_clip_id: None,
        });
        assert_eq!(out["ok"], false);
    }
}
