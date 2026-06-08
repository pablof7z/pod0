//! RAG / knowledge-search ranker backing the `podcast.knowledge.*`
//! action namespace.
//!
//! M5.3 wires the `podcast-knowledge` chunk store into the action path:
//!
//! * `IndexEpisode` chunks the Rust-stored transcript
//!   ([`PodcastStore::transcript_for`]) into ~200-word
//!   [`TranscriptChunk`]s and upserts them into [`KnowledgeStore`].
//! * `Search` does case-insensitive substring matching over both the
//!   episode title/description ([`collect_knowledge_matches`]) AND the
//!   indexed transcript chunks ([`merge_chunk_matches`]), deduped by
//!   episode and ranked by an early-position heuristic.
//!
//! Embeddings + the hybrid KNN/BM25/RRF/reranker ranker are a follow-up;
//! this baseline is lexical-only and needs no STT timing or vectors —
//! chunk timestamps are a `0.0` placeholder until the STT pipeline lands.
//!
//! Lives in its own module (rather than inline in
//! [`crate::host_op_handler`]) so:
//!
//! * `host_op_handler.rs` stays under the project's 500-line hard limit.
//! * The ranker can be unit-tested directly against a `PodcastStore` +
//!   `KnowledgeStore` without constructing an `NmpApp`.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use podcast_knowledge::bm25::{first_term_position, normalize_scores, tokenize, Bm25Index};
use podcast_knowledge::types::TranscriptChunk;
use podcast_knowledge::{KnowledgeChunk, KnowledgeStore};

use crate::ffi::actions::knowledge_module::KnowledgeAction;
use crate::ffi::projections::KnowledgeSearchResult;
use crate::store::PodcastStore;

/// Target words per transcript chunk for M5.3 indexing. Without STT
/// timing we can't window by time, so we window by word count — ~200
/// words is a reasonable RAG-chunk size (a few paragraphs of speech).
const CHUNK_TARGET_WORDS: usize = 200;

/// Apply a single `podcast.knowledge.*` action against the staged
/// result slot. Owns the lock discipline so the `host_op_handler`
/// dispatcher stays a thin router.
///
/// Returns the `{"ok":true,...}` envelope the kernel forwards to the
/// caller (matching every other host-op handler's contract — see D6).
pub(crate) fn handle_knowledge_action(
    action: KnowledgeAction,
    store: &Arc<Mutex<PodcastStore>>,
    slot: &Arc<Mutex<Vec<KnowledgeSearchResult>>>,
    knowledge_store: &Arc<Mutex<KnowledgeStore>>,
    rev: &Arc<AtomicU64>,
) -> serde_json::Value {
    match action {
        KnowledgeAction::Search { query } => {
            handle_search(query, store, slot, knowledge_store, rev)
        }
        KnowledgeAction::ClearResults => handle_clear_results(slot, rev),
        KnowledgeAction::IndexEpisode { episode_id } => {
            handle_index_episode(episode_id, store, knowledge_store, rev)
        }
    }
}

/// Chunk the Rust-stored transcript for `episode_id` into the RAG chunk
/// store (M5.3). Returns `{"ok":true,"status":"no_transcript"}` when no
/// transcript text has been stored yet, otherwise
/// `{"ok":true,"status":"indexed","chunk_count":N}`.
///
/// Chunks are word-windowed at [`CHUNK_TARGET_WORDS`]. Timing is a
/// placeholder (`start_secs`/`end_secs = 0.0`) because the plain-text
/// transcript carries no per-word timestamps — real timing arrives with
/// the STT pipeline. Upserts are idempotent on `(episode_id, chunk_index)`
/// (no `delete_episode` first), so re-indexing the same transcript
/// replaces in place rather than duplicating.
fn handle_index_episode(
    episode_id: String,
    store: &Arc<Mutex<PodcastStore>>,
    knowledge_store: &Arc<Mutex<KnowledgeStore>>,
    rev: &Arc<AtomicU64>,
) -> serde_json::Value {
    let text = match store.lock() {
        Ok(s) => match s.transcript_for(&episode_id) {
            Some(t) => t.to_owned(),
            None => return serde_json::json!({"ok": true, "status": "no_transcript"}),
        },
        Err(_) => return serde_json::json!({"ok": false, "error": "store poisoned"}),
    };

    let chunks = chunk_transcript_text(&episode_id, &text);
    let chunk_count = chunks.len();

    match knowledge_store.lock() {
        Ok(mut ks) => {
            // Delete all prior chunks for this episode before inserting the new
            // batch. Without this, a re-index with a shorter transcript leaves
            // stale trailing chunks (old indices N..M) that persist in search
            // results as if they were current content.
            ks.delete_episode(&episode_id);
            for chunk in chunks {
                ks.upsert(KnowledgeChunk::without_embedding(chunk));
            }
        }
        Err(_) => return serde_json::json!({"ok": false, "error": "knowledge_store poisoned"}),
    }

    rev.fetch_add(1, Ordering::Relaxed);
    serde_json::json!({"ok": true, "status": "indexed", "chunk_count": chunk_count})
}

/// Split `text` into ~[`CHUNK_TARGET_WORDS`]-word [`TranscriptChunk`]s with
/// sequential `chunk_index`. Whitespace-tokenised; the chunk text rejoins
/// the words with single spaces (lossy on original spacing, which is fine
/// for substring search). Empty / whitespace-only input yields no chunks.
fn chunk_transcript_text(episode_id: &str, text: &str) -> Vec<TranscriptChunk> {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() {
        return Vec::new();
    }
    words
        .chunks(CHUNK_TARGET_WORDS)
        .enumerate()
        .map(|(idx, window)| TranscriptChunk {
            episode_id: episode_id.to_owned(),
            chunk_index: idx as u32,
            start_secs: 0.0,
            end_secs: 0.0,
            text: window.join(" "),
            word_count: window.len() as u32,
        })
        .collect()
}

fn handle_search(
    query: String,
    store: &Arc<Mutex<PodcastStore>>,
    slot: &Arc<Mutex<Vec<KnowledgeSearchResult>>>,
    knowledge_store: &Arc<Mutex<KnowledgeStore>>,
    rev: &Arc<AtomicU64>,
) -> serde_json::Value {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        // Empty query clears the slot — same semantics as
        // `clear_results` so the UI doesn't have to special-case.
        return handle_clear_results(slot, rev);
    }
    // Title/description matches plus an `episode_id -> (podcast_title,
    // episode_title)` resolver built from the same library snapshot — the
    // chunk store only knows `episode_id`, so we resolve labels here while
    // we already hold the store lock.
    let (mut rows, labels) = match store.lock() {
        Ok(s) => (
            collect_knowledge_matches(&s, trimmed),
            build_episode_labels(&s),
        ),
        Err(_) => return serde_json::json!({"ok": false, "error": "store poisoned"}),
    };

    // Merge in transcript-chunk matches. Chunk hits carry a timestamp slot
    // and win the dedup over a title/description hit for the same episode.
    match knowledge_store.lock() {
        Ok(ks) => merge_chunk_matches(&mut rows, &ks, trimmed, &labels),
        Err(_) => return serde_json::json!({"ok": false, "error": "knowledge_store poisoned"}),
    }

    rows.sort_by(|a, b| {
        b.relevance_score
            .partial_cmp(&a.relevance_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    rows.truncate(KNOWLEDGE_SEARCH_TOP_K);

    match slot.lock() {
        Ok(mut out) => {
            *out = rows;
            rev.fetch_add(1, Ordering::Relaxed);
            serde_json::json!({"ok": true})
        }
        Err(_) => serde_json::json!({
            "ok": false,
            "error": "knowledge_search_results poisoned"
        }),
    }
}

fn handle_clear_results(
    slot: &Arc<Mutex<Vec<KnowledgeSearchResult>>>,
    rev: &Arc<AtomicU64>,
) -> serde_json::Value {
    match slot.lock() {
        Ok(mut out) => {
            if !out.is_empty() {
                out.clear();
                rev.fetch_add(1, Ordering::Relaxed);
            }
            serde_json::json!({"ok": true})
        }
        Err(_) => serde_json::json!({
            "ok": false,
            "error": "knowledge_search_results poisoned"
        }),
    }
}

/// Maximum results returned per `podcast.knowledge.search` (stub).
pub const KNOWLEDGE_SEARCH_TOP_K: usize = 10;
/// Snippet character budget surfaced in the projection.
pub const KNOWLEDGE_SNIPPET_MAX_CHARS: usize = 200;

/// Title/description ranker: tokenised BM25 over each episode's
/// `title + description` (one document per episode), with a `+0.2` boost
/// when a query term lands in the title, then truncated to
/// [`KNOWLEDGE_SEARCH_TOP_K`].
///
/// BM25 replaces the old whole-query substring matcher: a query like
/// `"machine learning"` now matches an episode that mentions both words in
/// any order, not only those that contain the contiguous phrase. Scores
/// are per-path normalised into `[0,1]` (top hit = 1.0) before the title
/// boost and final clamp, so they feed the projection's relevance bar
/// directly.
///
/// This is one of the two lexical signals [`handle_search`] merges; the
/// other is [`merge_chunk_matches`] over the indexed transcript chunks.
/// Kept as a standalone function (read-only `&PodcastStore`, owned
/// `Vec<KnowledgeSearchResult>`) so it stays directly unit-testable.
pub fn collect_knowledge_matches(store: &PodcastStore, query: &str) -> Vec<KnowledgeSearchResult> {
    let query_terms = tokenize(query);
    if query_terms.is_empty() {
        return Vec::new();
    }

    // One BM25 document per episode = title + description. Keep a parallel
    // table of the source rows so we can resolve a ranked doc index back to
    // its episode/podcast and decide the title-vs-description boost.
    struct Row<'a> {
        episode_id: String,
        episode_title: &'a str,
        podcast_title: &'a str,
        description: &'a str,
    }
    let mut rows: Vec<Row> = Vec::new();
    let mut corpus: Vec<String> = Vec::new();
    for (podcast, episodes) in store.subscribed_podcasts() {
        for ep in episodes {
            corpus.push(format!("{} {}", ep.title, ep.description));
            rows.push(Row {
                episode_id: ep.id.0.to_string(),
                episode_title: &ep.title,
                podcast_title: &podcast.title,
                description: &ep.description,
            });
        }
    }

    let index = Bm25Index::from_texts(&corpus);
    let ranked = normalize_scores(&index.rank(&query_terms));

    ranked
        .into_iter()
        .take(KNOWLEDGE_SEARCH_TOP_K)
        .map(|(doc, base)| {
            let row = &rows[doc];
            // Title hits weigh more than description-only hits.
            let title_hit = !tokenize(row.episode_title).is_empty()
                && query_terms
                    .iter()
                    .any(|t| row.episode_title.to_lowercase().contains(t.as_str()));
            let score = (base + if title_hit { 0.2 } else { 0.0 }).clamp(0.0, 1.0);
            // Anchor the snippet on the first matched term in whichever
            // field carries it: prefer the description (longer, more
            // context), fall back to the title.
            let desc_pos = first_term_position(row.description, &query_terms);
            let desc_has_term = query_terms
                .iter()
                .any(|t| row.description.to_lowercase().contains(t.as_str()));
            let snippet = if desc_has_term {
                build_snippet(row.description, desc_pos, 0)
            } else {
                let title_pos = first_term_position(row.episode_title, &query_terms);
                build_snippet(row.episode_title, title_pos, 0)
            };
            KnowledgeSearchResult {
                episode_id: row.episode_id.clone(),
                episode_title: row.episode_title.to_owned(),
                podcast_title: row.podcast_title.to_owned(),
                snippet,
                // Title/description carry no timestamp; chunk search
                // populates this from chunk metadata.
                start_secs: None,
                relevance_score: score,
            }
        })
        .collect()
}

/// Collect up to `limit` transcript chunks matching `query`, ranked by
/// BM25 over the in-scope chunk set (same engine as search). Returns
/// `(episode_id, text)` pairs carrying each chunk's full `text` (not the
/// 200-char snippet) so the wiki LLM gets real source material to
/// synthesize from, plus the owning episode id so the article can record
/// which episodes it drew from (M9 source attribution).
///
/// Scoped to `episode_ids`: the knowledge store is global across all
/// subscribed podcasts, but a per-podcast wiki article must only cite that
/// podcast's episodes. An empty `episode_ids` scope yields no chunks.
///
/// Unlike [`merge_chunk_matches`], this is library-agnostic: it doesn't
/// resolve episode labels or dedup by episode, because the wiki prompt
/// wants raw context excerpts, not UI rows. The attribution episode ids are
/// derived by the caller from the truncated (top-`limit`) result so they
/// reflect only the chunks that actually entered the LLM context window.
pub(crate) fn collect_chunk_texts_for_topic(
    knowledge_store: &KnowledgeStore,
    query: &str,
    episode_ids: &[String],
    limit: usize,
) -> Vec<(String, String)> {
    let query_terms = tokenize(query);
    if query_terms.is_empty() || episode_ids.is_empty() {
        return Vec::new();
    }
    let scope: std::collections::HashSet<&str> = episode_ids.iter().map(String::as_str).collect();
    // Build the BM25 corpus from the in-scope chunks only, so IDF and
    // length normalisation are computed against the same set we rank (a
    // per-podcast wiki article must only weigh that podcast's episodes).
    // `in_scope` carries `(episode_id, text)` so the ranked doc index maps
    // back to the owning episode for attribution.
    let in_scope: Vec<(&str, &str)> = knowledge_store
        .chunks
        .iter()
        .filter(|kc| scope.contains(kc.chunk.episode_id.as_str()))
        .map(|kc| (kc.chunk.episode_id.as_str(), kc.chunk.text.as_str()))
        .collect();
    if in_scope.is_empty() {
        return Vec::new();
    }
    let texts: Vec<&str> = in_scope.iter().map(|(_, text)| *text).collect();
    let index = Bm25Index::from_texts(&texts);
    index
        .rank(&query_terms)
        .into_iter()
        .take(limit)
        .map(|(doc, _)| {
            let (episode_id, text) = in_scope[doc];
            (episode_id.to_owned(), text.to_owned())
        })
        .collect()
}

/// Build an `episode_id -> (podcast_title, episode_title)` map from the
/// library so chunk matches (which only carry `episode_id`) can resolve
/// the labels [`KnowledgeSearchResult`] requires.
fn build_episode_labels(store: &PodcastStore) -> HashMap<String, (String, String)> {
    let mut map = HashMap::new();
    for (podcast, episodes) in store.subscribed_podcasts() {
        for ep in episodes {
            map.insert(
                ep.id.0.to_string(),
                (podcast.title.clone(), ep.title.clone()),
            );
        }
    }
    map
}

/// Case-insensitive substring search over the indexed transcript chunks,
/// merging hits into `rows`. A chunk hit:
///
/// * resolves its labels from `labels` (skipped if the episode isn't in
///   the library — e.g. unsubscribed since indexing),
/// * carries `start_secs` from the chunk (placeholder `0.0` until STT
///   timing lands), and
/// * replaces any existing title/description row for the same episode
///   (chunk matches are the higher-fidelity signal, and only chunk hits
///   can offer a seek timestamp).
///
/// Score uses the same early-in-the-haystack heuristic as title/desc
/// matches, plus a `0.1` chunk bonus so a transcript hit edges out a bare
/// description hit of equal position.
fn merge_chunk_matches(
    rows: &mut Vec<KnowledgeSearchResult>,
    knowledge_store: &KnowledgeStore,
    query: &str,
    labels: &HashMap<String, (String, String)>,
) {
    let query_terms = tokenize(query);
    if query_terms.is_empty() {
        return;
    }
    // BM25 over the whole chunk store, normalised per-path into [0,1].
    let corpus: Vec<&str> = knowledge_store
        .chunks
        .iter()
        .map(|kc| kc.chunk.text.as_str())
        .collect();
    let index = Bm25Index::from_texts(&corpus);
    let ranked = normalize_scores(&index.rank(&query_terms));

    for (doc, base) in ranked {
        let chunk = &knowledge_store.chunks[doc].chunk;
        let (podcast_title, episode_title) = match labels.get(&chunk.episode_id) {
            Some(pair) => pair.clone(),
            None => continue,
        };
        // Chunk bonus so a transcript hit edges out a bare title/desc hit
        // of equal normalised score.
        let score = (base + 0.1).clamp(0.0, 1.0);
        let pos = first_term_position(&chunk.text, &query_terms);
        let snippet = build_snippet(&chunk.text, pos, 0);
        let row = KnowledgeSearchResult {
            episode_id: chunk.episode_id.clone(),
            episode_title,
            podcast_title,
            snippet,
            start_secs: Some(chunk.start_secs),
            relevance_score: score,
        };
        // Dedup by episode: a chunk hit replaces a prior title/desc hit,
        // and a higher-scoring chunk replaces a lower-scoring one.
        if let Some(existing) = rows.iter_mut().find(|r| r.episode_id == row.episode_id) {
            if existing.start_secs.is_none() || row.relevance_score > existing.relevance_score {
                *existing = row;
            }
        } else {
            rows.push(row);
        }
    }
}

/// Build a snippet centered on `match_pos` within `text`, capped at
/// [`KNOWLEDGE_SNIPPET_MAX_CHARS`]. Char-boundary safe (`char_indices`)
/// so multi-byte UTF-8 (em-dashes, emoji) doesn't panic the slicer.
fn build_snippet(text: &str, match_pos: usize, match_len: usize) -> String {
    if text.chars().count() <= KNOWLEDGE_SNIPPET_MAX_CHARS {
        return text.trim().to_owned();
    }
    let target_center = match_pos + match_len / 2;
    let half = KNOWLEDGE_SNIPPET_MAX_CHARS / 2;
    let raw_start = target_center.saturating_sub(half);
    // Snap to the nearest char boundary at or before `raw_start`.
    let start = text
        .char_indices()
        .map(|(i, _)| i)
        .take_while(|&i| i <= raw_start)
        .last()
        .unwrap_or(0);
    let mut snippet = String::with_capacity(KNOWLEDGE_SNIPPET_MAX_CHARS + 2);
    if start > 0 {
        snippet.push('…');
    }
    let mut taken = 0usize;
    for (i, ch) in text.char_indices() {
        if i < start {
            continue;
        }
        if taken >= KNOWLEDGE_SNIPPET_MAX_CHARS {
            snippet.push('…');
            break;
        }
        snippet.push(ch);
        taken += 1;
    }
    snippet
}

#[cfg(test)]
#[path = "knowledge_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "knowledge_indexing_tests.rs"]
mod indexing_tests;
