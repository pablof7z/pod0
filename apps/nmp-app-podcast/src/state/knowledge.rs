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

#[path = "knowledge_search.rs"]
mod knowledge_search;

use std::path::Path;
use std::sync::{Arc, Mutex};

use podcast_knowledge::sqlite::KnowledgeSqliteStore;
use podcast_knowledge::KnowledgeStore;

use crate::ffi::actions::knowledge_module::KnowledgeAction;
use crate::ffi::projections::KnowledgeSearchResult;
use crate::state::slot::{Derived, Session};
use crate::state::{Infra, Slot};
use crate::store::PodcastStore;

use std::sync::atomic::{AtomicBool, Ordering};

/// Process-global guard so a misconfigured `embeddings_model` (e.g. a bare chat
/// model string with no `/` and not ending in `:cloud`) logs the "not a usable
/// embedding model" warning at most ONCE per process instead of once per indexed
/// episode (a bulk re-index would otherwise spam the log).
/// Gates both the ingest-task and backfill no-op branches.
static EMBED_MODEL_WARNED: AtomicBool = AtomicBool::new(false);

/// Emit the "not a usable embedding model" warning at most once per process.
fn warn_unusable_embedding_model_once(model: &str) {
    if !EMBED_MODEL_WARNED.swap(true, Ordering::Relaxed) {
        log::warn!(
            "[knowledge] embeddings_model '{model}' is not a usable embedding model \
             — skipping embed (NULL rows remain, BM25 works). This warning fires \
             once per process."
        );
    }
}

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
    /// SQLite durable sidecar.  `None` until `set_data_dir` is called.
    /// Interior-mutable so `set_data_dir` can take `&self` like all other
    /// methods on this type.
    sqlite: Arc<Mutex<Option<KnowledgeSqliteStore>>>,
}

impl KnowledgeState {
    /// Production constructor — called from `PodcastAppState::new`.
    pub fn new(infra: Infra, store: Arc<Mutex<PodcastStore>>) -> Self {
        Self {
            results: Slot::new(Vec::new()),
            index: Slot::new(KnowledgeStore::new()),
            infra,
            store,
            sqlite: Arc::new(Mutex::new(None)),
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

    /// Bind the knowledge sidecar to `dir/knowledge.sqlite`.
    ///
    /// Opens (or creates) the SQLite file, runs migrations, then cold-loads
    /// all persisted chunks into the in-memory `KnowledgeStore`.  Called
    /// from `nmp_app_podcast_set_data_dir` after the main library store is
    /// bound — same data-dir, separate sidecar file.
    ///
    /// Returns the number of chunks reloaded from disk so the FFI layer can
    /// decide whether to bump the snapshot rev.
    ///
    /// If the file is corrupt the sidecar degrades to an in-memory no-op
    /// (quarantine handled inside `KnowledgeSqliteStore::open`).  Errors
    /// from the SQLite layer never propagate outward (D6).
    pub fn set_data_dir(&self, dir: &Path) -> usize {
        let sqlite_path = dir.join("knowledge.sqlite");
        let sq = KnowledgeSqliteStore::open(&sqlite_path);
        let chunks = sq.load_all();
        let count = chunks.len();

        // Seed the in-memory working set with the persisted chunks.
        if let Ok(mut ks) = self.index.lock() {
            ks.upsert_many(chunks);
        }

        // Store the live SQLite handle so write-through can reach it.
        if let Ok(mut guard) = self.sqlite.lock() {
            *guard = Some(sq);
        }

        // Kick off paced backfill for any NULL-embedding rows from prior sessions.
        // Off-actor — returns immediately; halts on provider error + resumes next cold start.
        if count > 0 {
            self.backfill_embeddings();
        }

        count
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

        // Build KnowledgeChunk wrappers once (always NULL embedding on the sync path;
        // the off-actor embed task will fill them in asynchronously).
        let kchunks: Vec<KnowledgeChunk> = chunks
            .into_iter()
            .map(KnowledgeChunk::without_embedding)
            .collect();

        match self.index.lock() {
            Ok(mut ks) => {
                // Delete all prior chunks for this episode before inserting the
                // new batch — without this a re-index with a shorter transcript
                // leaves stale trailing chunks.
                ks.delete_episode(&episode_id);
                for chunk in &kchunks {
                    ks.upsert(chunk.clone());
                }
            }
            Err(_) => return serde_json::json!({"ok": false, "error": "knowledge_store poisoned"}),
        }

        // Write-through to SQLite atomically (D6 — errors silently ignored).
        // Guard released before the infra.bump() below (lock-order rule §6.2).
        if let Ok(guard) = self.sqlite.lock() {
            if let Some(sq) = guard.as_ref() {
                let _ = sq.replace_episode_chunks(&episode_id, &kchunks);
            }
        }

        // Spawn off-actor embed task (D8: never block the actor thread).
        // Clone Arcs before entering the async block.
        let sqlite_c = Arc::clone(&self.sqlite);
        let index_c = self.index.share();
        let store_c = Arc::clone(&self.store);
        let infra_c = self.infra.clone();
        let ep_id = episode_id.clone();

        self.infra.runtime.spawn(async move {
            // Resolve provider + model from settings.
            let (provider, model) = {
                let Ok(s) = store_c.lock() else { return };
                let model_str = s.embeddings_model().to_owned();
                let provider = if model_str.contains('/') {
                    crate::llm::provider_transport::ProviderKind::OpenRouter
                } else if model_str.ends_with(":cloud") {
                    crate::llm::provider_transport::ProviderKind::Ollama
                } else {
                    warn_unusable_embedding_model_once(&model_str);
                    return;
                };
                (provider, model_str)
            };

            // Collect the texts we need to embed (from in-memory index).
            let texts: Vec<(u32, String)> = {
                let Ok(ks) = index_c.lock() else { return };
                ks.chunks_for_episode(&ep_id)
                    .into_iter()
                    .map(|c| (c.chunk.chunk_index, c.chunk.text.clone()))
                    .collect()
            };
            if texts.is_empty() {
                return;
            }

            // Call the embed transport.
            let intent = crate::llm::provider_transport::EmbeddingIntent {
                provider,
                model: model.clone(),
                input: texts.iter().map(|(_, t)| t.clone()).collect(),
                dimensions: Some(podcast_knowledge::EXPECTED_EMBEDDING_DIM),
            };
            let result =
                match crate::llm::provider_transport::embed(Arc::clone(&store_c), intent).await {
                    Ok(r) => r,
                    Err(e) => {
                        log::warn!("[knowledge] embed call failed for episode {ep_id}: {e}");
                        return;
                    }
                };

            // Validate dimensions and attach.
            for ((chunk_index, chunk_text), raw_embedding) in
                texts.iter().zip(result.embeddings.iter())
            {
                if raw_embedding.len() != podcast_knowledge::EXPECTED_EMBEDDING_DIM {
                    log::warn!(
                        "[knowledge] episode {ep_id} chunk {chunk_index}: expected dim {}, \
                         got {} — skipping",
                        podcast_knowledge::EXPECTED_EMBEDDING_DIM,
                        raw_embedding.len()
                    );
                    continue;
                }
                let ev = podcast_knowledge::EmbeddingVector::new(raw_embedding.clone());
                // Attach to in-memory index.
                if let Ok(mut ks) = index_c.lock() {
                    ks.attach_embedding(&ep_id, *chunk_index, ev.clone());
                }
                // Persist to SQLite, guarded on the captured text so a concurrent
                // re-ingest can't bind a stale embedding to changed text.
                if let Ok(guard) = sqlite_c.lock() {
                    if let Some(sq) = guard.as_ref() {
                        if let Err(e) = sq.upsert_embedding(&ep_id, *chunk_index, chunk_text, &ev) {
                            log::warn!("[knowledge] upsert_embedding failed: {e}");
                        }
                    }
                }
            }
            // Drop all guards before bump (lock-order §6.2).
            infra_c.bump();
        });

        // Drop guard before bump (lock-order rule §6.2).
        self.infra.bump();
        serde_json::json!({"ok": true, "status": "indexed", "chunk_count": chunk_count})
    }

    /// Invoked from `set_data_dir` after cold-load to schedule paced embed tasks
    /// for any NULL-embedding chunks already in SQLite.
    /// Spawn body lives in `knowledge_search.rs` (file-length budget).
    fn backfill_embeddings(&self) {
        knowledge_search::spawn_backfill_embeddings(
            Arc::clone(&self.sqlite),
            self.index.share(),
            Arc::clone(&self.store),
            self.infra.clone(),
        );
    }

    fn search(&self, query: String) -> serde_json::Value {
        let trimmed = query.trim();
        if trimmed.is_empty() {
            return self.clear_results();
        }
        // Keep a trimmed owned copy for the async spawn below.
        let trimmed_owned = trimmed.to_owned();

        // ── SYNC BM25 baseline (degrade-safe, ~0 ms) ────────────────────────
        // Collect BM25 rows synchronously and commit them as the first result
        // set. The user sees lexical hits immediately while the async embed
        // round-trip completes in the background.
        let (mut rows, labels) = match self.store.lock() {
            Ok(s) => (
                crate::knowledge::collect_knowledge_matches(&s, &trimmed_owned),
                crate::knowledge::build_episode_labels_pub(&s),
            ),
            Err(_) => return serde_json::json!({"ok": false, "error": "store poisoned"}),
        };

        match self.index.lock() {
            Ok(ks) => {
                crate::knowledge::merge_chunk_matches_pub(&mut rows, &ks, &trimmed_owned, &labels)
            }
            Err(_) => return serde_json::json!({"ok": false, "error": "knowledge_store poisoned"}),
        }

        rows.sort_by(|a, b| {
            b.relevance_score
                .partial_cmp(&a.relevance_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        rows.truncate(crate::knowledge::KNOWLEDGE_SEARCH_TOP_K);

        // Commit BM25 results and emit first bump (D6 degrade baseline).
        match self.results.lock() {
            Ok(mut out) => {
                *out = rows.clone();
                drop(out); // guard released before bump (lock-order §6.2)
                self.infra.bump();
            }
            Err(_) => {
                return serde_json::json!({
                    "ok": false,
                    "error": "knowledge_search_results poisoned"
                })
            }
        }

        // ── ASYNC semantic refinement (off-actor, D8) ────────────────────────
        // The spawn body lives in `knowledge_search.rs` (file-length budget).
        knowledge_search::spawn_semantic_search(
            trimmed_owned,
            Arc::clone(&self.store),
            self.index.share(),
            self.results.share(),
            self.infra.clone(),
        );

        serde_json::json!({"ok": true})
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
#[path = "knowledge_state_tests.rs"]
mod tests;
