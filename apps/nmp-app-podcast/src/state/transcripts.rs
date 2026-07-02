//! Transcripts substate — Step 5b of the god-root consolidation.
//!
//! Owns the single slot that was previously mirrored between
//! `PodcastHandle` and `PodcastHostOpHandler`:
//!
//! * `cache` — per-episode transcript entries keyed by `EpisodeId` string.
//!   **Session** durability (re-fetched on the next launch; avoids growing
//!   `podcasts.json` with potentially large transcript bodies).
//!
//! The free function `crate::transcript::handle_fetch_transcript` is
//! re-exposed as `TranscriptsState::handle_fetch` so the router arm
//! calls `self.state.transcripts.handle_fetch(...)` instead of
//! constructing the Arcs inline.
//!
//! ## Snapshot path
//!
//! `build_podcast_update` previously read `handle.transcripts` directly.
//! After migration it calls `handle.state.transcripts.snapshot()` which
//! returns the same `HashMap<String, Vec<TranscriptEntry>>` clone.
//! The `snapshot_library::build_library_snapshot` helper receives this
//! snapshot and does `transcript_for` lookups inside — byte-identical
//! to the pre-migration path.
//!
//! ## File-length ceiling
//!
//! AGENTS.md hard limit is 500 lines.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use podcast_feeds::http::HttpRequest;

use crate::ffi::projections::TranscriptEntry;
use crate::state::slot::Session;
use crate::state::{Infra, Slot};
use crate::store::PodcastStore;

/// Transcripts feature substate.
///
/// Constructed once in `PodcastAppState::new` and referenced via
/// `state.transcripts` on both seams.
pub struct TranscriptsState {
    /// Per-episode transcript cache.  Keyed by the string form of
    /// `EpisodeId`.  Session durability.
    pub cache: Slot<HashMap<String, Vec<TranscriptEntry>>, Session>,
    /// Rev + signal + runtime.
    infra: Infra,
    /// The canonical persisted library — read by `handle_fetch_transcript`
    /// to look up the publisher transcript URL and kind.
    store: Arc<Mutex<PodcastStore>>,
}

impl TranscriptsState {
    /// Production constructor — called from `PodcastAppState::new`.
    pub fn new(infra: Infra, store: Arc<Mutex<PodcastStore>>) -> Self {
        Self {
            cache: Slot::new(HashMap::new()),
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

    /// Clone the current transcript cache for the snapshot projection.
    ///
    /// `build_podcast_update` calls this instead of locking
    /// `handle.transcripts` directly.  The returned map is passed to
    /// `snapshot_library::build_library_snapshot` which does
    /// `transcript_for` lookups — byte-identical to the pre-migration path.
    pub fn snapshot(&self) -> HashMap<String, Vec<TranscriptEntry>> {
        self.cache
            .lock()
            .ok()
            .map(|t| t.clone())
            .unwrap_or_default()
    }

    // ── Action handler ────────────────────────────────────────────────────

    /// Fetch, parse, and cache a transcript for `episode_id`.
    ///
    /// Wraps `crate::transcript::handle_fetch_transcript` so the router
    /// arm calls `self.state.transcripts.handle_fetch(episode_id, fetch)`
    /// instead of constructing the Arcs inline.
    ///
    /// The `fetch` closure is injected by the call site; tests inject a
    /// deterministic stub.
    pub fn handle_fetch(
        &self,
        episode_id: String,
        fetch: impl FnOnce(&HttpRequest) -> Result<podcast_feeds::http::HttpResult, String>,
    ) -> serde_json::Value {
        // `handle_fetch_transcript` bumps `rev` internally when it stores
        // the transcript.  It uses the raw `AtomicU64` (pre-`infra` style)
        // on the same Arc, so the snapshot sees the bump.
        crate::transcript::handle_fetch_transcript(
            &self.store,
            &self.cache.share(),
            &self.infra.rev,
            episode_id,
            fetch,
        )
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use podcast_core::{Episode, Podcast, TranscriptKind};
    use url::Url;

    use crate::store::PodcastStore;

    use super::*;

    fn make_store_with_transcript_episode() -> Arc<Mutex<PodcastStore>> {
        let mut store = PodcastStore::new();
        let podcast = Podcast::new("Transcript Show");
        let pid = podcast.id;
        let ep = Episode::new(
            pid,
            "https://example.com/feed.xml",
            "guid-tr-1",
            "Transcribed Episode",
            Url::parse("https://example.com/ep.mp3").unwrap(),
            chrono::Utc::now(),
        );
        store.subscribe(podcast, vec![ep]);
        Arc::new(Mutex::new(store))
    }

    #[test]
    fn snapshot_returns_empty_when_no_transcripts() {
        let state = TranscriptsState::for_test(Arc::new(Mutex::new(PodcastStore::new())));
        assert!(state.snapshot().is_empty());
    }

    #[test]
    fn handle_fetch_not_available_when_no_url() {
        // Episode has no publisher transcript URL → `not_available`.
        let store = make_store_with_transcript_episode();
        let state = TranscriptsState::for_test(store);
        // The episode exists but has no publisher transcript URL, so the
        // free function returns `not_available` without calling `fetch`.
        let ep_id = {
            let s = state.store.lock().unwrap();
            s.all_podcasts()
                .into_iter()
                .flat_map(|(_, eps)| eps.iter())
                .next()
                .unwrap()
                .id
                .0
                .to_string()
        };
        let out = state.handle_fetch(ep_id, |_req| {
            panic!("fetch should not be called when no URL");
        });
        assert_eq!(out["ok"], true);
        assert_eq!(out["status"], "not_available");
    }

    #[test]
    fn handle_fetch_stores_parsed_transcript() {
        use podcast_feeds::http::HttpResult;

        let mut store = PodcastStore::new();
        let podcast = Podcast::new("Fetch Show");
        let pid = podcast.id;
        let mut ep = Episode::new(
            pid,
            "https://example.com/feed.xml",
            "guid-fetch-1",
            "Episode With VTT",
            Url::parse("https://example.com/ep.mp3").unwrap(),
            chrono::Utc::now(),
        );
        // Attach a publisher transcript URL.
        ep.publisher_transcript_url = Some(Url::parse("https://cdn.example.com/ep.vtt").unwrap());
        ep.publisher_transcript_type = Some(TranscriptKind::Vtt);
        store.subscribe(podcast, vec![ep]);
        let store = Arc::new(Mutex::new(store));
        let ep_id = {
            let s = store.lock().unwrap();
            s.all_podcasts()
                .into_iter()
                .flat_map(|(_, eps)| eps.iter())
                .next()
                .unwrap()
                .id
                .0
                .to_string()
        };
        let state = TranscriptsState::for_test(store);
        let rev0 = state.infra.rev();

        let vtt_body = "WEBVTT\n\n00:00:00.000 --> 00:00:05.000\nHello world";
        let out = state.handle_fetch(ep_id.clone(), |_req| {
            Ok(HttpResult::Ok {
                body: vtt_body.to_owned(),
                status_code: 200,
                headers: vec![],
                body_base64: None,
            })
        });
        assert_eq!(out["ok"], true);
        assert_eq!(out["status"], "fetched");
        assert!(state.infra.rev() > rev0, "must bump rev after storing");
        let snap = state.snapshot();
        assert!(snap.contains_key(&ep_id), "episode must be in cache");
        assert!(!snap[&ep_id].is_empty(), "must have parsed entries");
    }
}
