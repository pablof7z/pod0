//! Knowledge substate — Step 1 of the god-root consolidation.
//!
//! Owns the two slots that were previously mirrored between
//! `PodcastHandle` and `PodcastHostOpHandler`:
//!
//! * `results` — transient RAG search results projected into
//!   `PodcastUpdate.knowledge_search_results`.  **Session** durability
//!   (evaporates on restart).
//! * `index` — in-memory RAG chunk store, re-indexable from persisted
//!   transcripts.  **Derived** durability.
//!
//! The free function `crate::knowledge::handle_knowledge_action(action,
//! &store, &results, &index, &rev)` is replaced by `KnowledgeState::handle`,
//! which has the same contract (returns the `{"ok":…}` envelope the kernel
//! forwards to the caller) but reaches all its dependencies through `&self`
//! instead of receiving them as extra parameters.
//!
//! Pure helpers (`collect_knowledge_matches`, `merge_chunk_matches`,
//! `chunk_transcript_text`, BM25 ranker) **stay free functions** in
//! `crate::knowledge` — only the Arc-threading shell moves here.
//!
//! ## File-length ceiling
//!
//! AGENTS.md hard limit is 500 lines.  This file stays well below that;
//! if additional Knowledge actions are added, split into a sibling
//! `knowledge_actions.rs` before reaching the 300-line soft limit.

use std::sync::{Arc, Mutex};

use podcast_knowledge::KnowledgeStore;

use crate::ffi::actions::knowledge_module::KnowledgeAction;
use crate::ffi::projections::KnowledgeSearchResult;
use crate::state::slot::{Derived, Session};
use crate::state::{Infra, Slot};
use crate::store::PodcastStore;

/// Knowledge feature substate.
///
/// Constructed once in `PodcastAppState::new` and referenced via
/// `state.knowledge` on both seams.  All methods are `&self` — no
/// external state needed because the slots and infra are self-contained.
pub struct KnowledgeState {
    /// Transient RAG / knowledge-search results projected into
    /// `PodcastUpdate.knowledge_search_results`.  Session durability.
    pub results: Slot<Vec<KnowledgeSearchResult>, Session>,
    /// In-memory RAG chunk store (M5.3).  Re-indexable from persisted
    /// transcripts.  Derived durability.
    pub index: Slot<KnowledgeStore, Derived>,
    /// Rev + signal + runtime (cloned from `PodcastAppState::infra`).
    infra: Infra,
    /// The canonical persisted library.  Knowledge reads transcripts from
    /// here for indexing.
    store: Arc<Mutex<PodcastStore>>,
}

impl KnowledgeState {
    /// Production constructor — called from `PodcastAppState::new`.
    pub fn new(infra: Infra, store: Arc<Mutex<PodcastStore>>) -> Self {
        Self {
            results: Slot::new(Vec::new()),
            index: Slot::new(KnowledgeStore::new()),
            infra,
            store,
        }
    }

    /// Test constructor — builds a `KnowledgeState` without an `NmpApp`.
    ///
    /// Pass a pre-seeded `PodcastStore` to exercise indexing / search
    /// against real episode data without constructing a full handle.
    #[cfg(test)]
    pub fn for_test(store: Arc<Mutex<PodcastStore>>) -> Self {
        Self::new(Infra::for_test(), store)
    }

    /// Return the `Arc<Mutex<KnowledgeStore>>` for legacy callers that
    /// hold a direct `Arc` (e.g. `WikiState` which shares the index).
    ///
    /// Used by `state/mod.rs` Step-2 Wiki migration and by any observer
    /// that needs the bare `Arc` via `.share()`.
    pub fn index_arc(&self) -> Arc<Mutex<KnowledgeStore>> {
        self.index.share()
    }

    // ── Snapshot projection ───────────────────────────────────────────────

    /// Clone the current results for the snapshot projection.
    ///
    /// `build_podcast_update` calls this instead of locking
    /// `handle.knowledge_search_results` directly.  Byte-identical output
    /// is asserted by the golden test.
    pub fn results_snapshot(&self) -> Vec<KnowledgeSearchResult> {
        self.results.lock().ok().map(|r| r.clone()).unwrap_or_default()
    }

    // ── Action handler ────────────────────────────────────────────────────

    /// Route a single `podcast.knowledge.*` action.
    ///
    /// Replaces `crate::knowledge::handle_knowledge_action`.
    /// Returns the `{"ok":…}` envelope the kernel forwards to the caller
    /// (D6 contract — same as every other host-op handler).
    pub fn handle(&self, action: KnowledgeAction) -> serde_json::Value {
        match action {
            KnowledgeAction::Search { query } => self.search(query),
            KnowledgeAction::ClearResults => self.clear_results(),
            KnowledgeAction::IndexEpisode { episode_id } => self.index_episode(episode_id),
        }
    }

    // ── Private action bodies ─────────────────────────────────────────────

    fn index_episode(&self, episode_id: String) -> serde_json::Value {
        use podcast_knowledge::KnowledgeChunk;

        let text = match self.store.lock() {
            Ok(s) => match s.transcript_for(&episode_id) {
                Some(t) => t.to_owned(),
                None => return serde_json::json!({"ok": true, "status": "no_transcript"}),
            },
            Err(_) => return serde_json::json!({"ok": false, "error": "store poisoned"}),
        };

        let chunks = crate::knowledge::chunk_transcript_text(&episode_id, &text);
        let chunk_count = chunks.len();

        match self.index.lock() {
            Ok(mut ks) => {
                // Delete all prior chunks for this episode before inserting the
                // new batch — without this a re-index with a shorter transcript
                // leaves stale trailing chunks.
                ks.delete_episode(&episode_id);
                for chunk in chunks {
                    ks.upsert(KnowledgeChunk::without_embedding(chunk));
                }
            }
            Err(_) => return serde_json::json!({"ok": false, "error": "knowledge_store poisoned"}),
        }

        // Drop guard before bump (lock-order rule §6.2).
        self.infra.bump();
        serde_json::json!({"ok": true, "status": "indexed", "chunk_count": chunk_count})
    }

    fn search(&self, query: String) -> serde_json::Value {
        let trimmed = query.trim();
        if trimmed.is_empty() {
            return self.clear_results();
        }

        let (mut rows, labels) = match self.store.lock() {
            Ok(s) => (
                crate::knowledge::collect_knowledge_matches(&s, trimmed),
                crate::knowledge::build_episode_labels_pub(&s),
            ),
            Err(_) => return serde_json::json!({"ok": false, "error": "store poisoned"}),
        };

        match self.index.lock() {
            Ok(ks) => crate::knowledge::merge_chunk_matches_pub(&mut rows, &ks, trimmed, &labels),
            Err(_) => return serde_json::json!({"ok": false, "error": "knowledge_store poisoned"}),
        }

        rows.sort_by(|a, b| {
            b.relevance_score
                .partial_cmp(&a.relevance_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        rows.truncate(crate::knowledge::KNOWLEDGE_SEARCH_TOP_K);

        match self.results.lock() {
            Ok(mut out) => {
                *out = rows;
                // Drop guard before bump.
                drop(out);
                self.infra.bump();
                serde_json::json!({"ok": true})
            }
            Err(_) => serde_json::json!({
                "ok": false,
                "error": "knowledge_search_results poisoned"
            }),
        }
    }

    fn clear_results(&self) -> serde_json::Value {
        match self.results.lock() {
            Ok(mut out) => {
                let changed = !out.is_empty();
                if changed {
                    out.clear();
                }
                drop(out);
                self.infra.bump_if(changed);
                serde_json::json!({"ok": true})
            }
            Err(_) => serde_json::json!({
                "ok": false,
                "error": "knowledge_search_results poisoned"
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use podcast_core::{Episode, Podcast, PodcastId};
    use url::Url;
    use uuid::Uuid;

    use crate::store::PodcastStore;

    use super::*;

    fn make_episode(podcast_id: PodcastId, title: &str, description: &str) -> Episode {
        let mut ep = Episode::new(
            podcast_id,
            "https://example.com/feed.xml",
            format!("guid-{}", Uuid::new_v4()),
            title,
            Url::parse("https://example.com/audio.mp3").unwrap(),
            chrono::Utc::now(),
        );
        ep.description = description.to_owned();
        ep
    }

    fn shared(store: PodcastStore) -> Arc<Mutex<PodcastStore>> {
        Arc::new(Mutex::new(store))
    }

    #[test]
    fn empty_search_clears_results() {
        let state = KnowledgeState::for_test(shared(PodcastStore::new()));
        // Seed some dummy results.
        state.results.lock().unwrap().push(KnowledgeSearchResult {
            episode_id: "ep-1".to_owned(),
            ..Default::default()
        });
        let before = state.infra.rev();
        let out = state.handle(KnowledgeAction::Search {
            query: "  ".to_owned(),
        });
        assert_eq!(out["ok"], true);
        assert!(state.results_snapshot().is_empty());
        assert!(state.infra.rev() > before, "empty search must bump rev");
    }

    #[test]
    fn search_finds_matching_episode() {
        let mut store = PodcastStore::new();
        let podcast = Podcast::new("Tech Talk");
        let id = podcast.id;
        let ep = make_episode(id, "machine learning deep dive", "learn about ML");
        store.subscribe(podcast, vec![ep.clone()]);

        let state = KnowledgeState::for_test(shared(store));
        let out = state.handle(KnowledgeAction::Search {
            query: "machine learning".to_owned(),
        });
        assert_eq!(out["ok"], true);
        let results = state.results_snapshot();
        assert!(!results.is_empty());
        assert_eq!(results[0].episode_id, ep.id.0.to_string());
    }

    #[test]
    fn clear_results_bumps_rev_only_when_nonempty() {
        let state = KnowledgeState::for_test(shared(PodcastStore::new()));
        let rev0 = state.infra.rev();
        // Clear when already empty — no bump.
        let out = state.handle(KnowledgeAction::ClearResults);
        assert_eq!(out["ok"], true);
        assert_eq!(state.infra.rev(), rev0, "clear of empty must NOT bump rev");

        // Seed a result then clear.
        state.results.lock().unwrap().push(KnowledgeSearchResult {
            episode_id: "ep-1".to_owned(),
            ..Default::default()
        });
        let out2 = state.handle(KnowledgeAction::ClearResults);
        assert_eq!(out2["ok"], true);
        assert!(state.infra.rev() > rev0, "clear of non-empty must bump rev");
    }

    #[test]
    fn index_episode_without_transcript_no_error() {
        let state = KnowledgeState::for_test(shared(PodcastStore::new()));
        let out = state.handle(KnowledgeAction::IndexEpisode {
            episode_id: "missing".to_owned(),
        });
        assert_eq!(out["ok"], true);
        assert_eq!(out["status"], "no_transcript");
    }

    #[test]
    fn index_episode_chunks_and_bumps_rev() {
        let mut store = PodcastStore::new();
        let text = (0..300)
            .map(|i| format!("word{i}"))
            .collect::<Vec<_>>()
            .join(" ");
        store.set_transcript("ep-chunked".to_owned(), text);

        let state = KnowledgeState::for_test(shared(store));
        let rev0 = state.infra.rev();
        let out = state.handle(KnowledgeAction::IndexEpisode {
            episode_id: "ep-chunked".to_owned(),
        });
        assert_eq!(out["ok"], true);
        assert_eq!(out["status"], "indexed");
        assert!(out["chunk_count"].as_u64().unwrap() > 0);
        assert!(state.infra.rev() > rev0);
    }
}
