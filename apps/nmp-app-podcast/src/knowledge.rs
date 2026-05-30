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

/// Title/description ranker: case-insensitive substring match over each
/// episode's title + description, sorted by [`score_match`] and truncated
/// to [`KNOWLEDGE_SEARCH_TOP_K`].
///
/// This is one of the two lexical signals [`handle_search`] merges; the
/// other is [`merge_chunk_matches`] over the indexed transcript chunks.
/// Kept as a standalone function (read-only `&PodcastStore`, owned
/// `Vec<KnowledgeSearchResult>`) so it stays directly unit-testable.
pub fn collect_knowledge_matches(
    store: &PodcastStore,
    query: &str,
) -> Vec<KnowledgeSearchResult> {
    let needle = query.to_lowercase();
    if needle.is_empty() {
        return Vec::new();
    }
    let mut scored: Vec<(f32, KnowledgeSearchResult)> = Vec::new();
    for (podcast, episodes) in store.all_podcasts() {
        for ep in episodes {
            let title_lc = ep.title.to_lowercase();
            let desc_lc = ep.description.to_lowercase();
            let title_pos = title_lc.find(&needle);
            let desc_pos = desc_lc.find(&needle);
            if title_pos.is_none() && desc_pos.is_none() {
                continue;
            }
            // Title hits weigh more than description hits.
            let score = match (title_pos, desc_pos) {
                (Some(p), _) => score_match(p, title_lc.len()) + 0.2,
                (None, Some(p)) => score_match(p, desc_lc.len()),
                _ => 0.0,
            };
            let snippet = match desc_pos {
                Some(p) => build_snippet(&ep.description, p, needle.len()),
                None => build_snippet(&ep.title, title_pos.unwrap_or(0), needle.len()),
            };
            scored.push((
                score.clamp(0.0, 1.0),
                KnowledgeSearchResult {
                    episode_id: ep.id.0.to_string(),
                    episode_title: ep.title.clone(),
                    podcast_title: podcast.title.clone(),
                    snippet,
                    // The stub only matches against title + description,
                    // neither of which carries a timestamp. Real chunk
                    // search will populate this from the chunk metadata.
                    start_secs: None,
                    relevance_score: score.clamp(0.0, 1.0),
                },
            ));
        }
    }
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    scored
        .into_iter()
        .take(KNOWLEDGE_SEARCH_TOP_K)
        .map(|(_, r)| r)
        .collect()
}

/// Collect the full text of up to `limit` transcript chunks matching
/// `query` (case-insensitive substring), ranked by the same early-position
/// [`score_match`] heuristic used by search. Returns the chunks' full
/// `text` (not the 200-char snippet) so the wiki LLM gets real source
/// material to synthesize from (M5.6-wiki RAG context).
///
/// Scoped to `episode_ids`: the knowledge store is global across all
/// subscribed podcasts, but a per-podcast wiki article must only cite that
/// podcast's episodes. An empty `episode_ids` scope yields no chunks.
///
/// Unlike [`merge_chunk_matches`], this is library-agnostic: it doesn't
/// resolve episode labels or dedup by episode, because the wiki prompt
/// wants raw context excerpts, not UI rows.
pub(crate) fn collect_chunk_texts_for_topic(
    knowledge_store: &KnowledgeStore,
    query: &str,
    episode_ids: &[String],
    limit: usize,
) -> Vec<String> {
    let needle = query.to_lowercase();
    if needle.is_empty() || episode_ids.is_empty() {
        return Vec::new();
    }
    let scope: std::collections::HashSet<&str> =
        episode_ids.iter().map(String::as_str).collect();
    let mut scored: Vec<(f32, String)> = Vec::new();
    for kc in &knowledge_store.chunks {
        let chunk = &kc.chunk;
        if !scope.contains(chunk.episode_id.as_str()) {
            continue;
        }
        let text_lc = chunk.text.to_lowercase();
        if let Some(pos) = text_lc.find(&needle) {
            let score = score_match(pos, text_lc.len());
            scored.push((score, chunk.text.clone()));
        }
    }
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    scored.into_iter().take(limit).map(|(_, t)| t).collect()
}

/// Build an `episode_id -> (podcast_title, episode_title)` map from the
/// library so chunk matches (which only carry `episode_id`) can resolve
/// the labels [`KnowledgeSearchResult`] requires.
fn build_episode_labels(store: &PodcastStore) -> HashMap<String, (String, String)> {
    let mut map = HashMap::new();
    for (podcast, episodes) in store.all_podcasts() {
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
    let needle = query.to_lowercase();
    if needle.is_empty() {
        return;
    }
    for kc in &knowledge_store.chunks {
        let chunk = &kc.chunk;
        let text_lc = chunk.text.to_lowercase();
        let pos = match text_lc.find(&needle) {
            Some(p) => p,
            None => continue,
        };
        let (podcast_title, episode_title) = match labels.get(&chunk.episode_id) {
            Some(pair) => pair.clone(),
            None => continue,
        };
        let score = (score_match(pos, text_lc.len()) + 0.1).clamp(0.0, 1.0);
        let snippet = build_snippet(&chunk.text, pos, needle.len());
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

/// Cheap "how early in the haystack did the needle land" heuristic
/// mapped to a `0.0..=1.0` relevance score. An early match against a
/// long text scores higher than a late match against the same text;
/// any match scores at least 0.1 so the UI's relevance bar is never
/// invisible.
fn score_match(position: usize, total_len: usize) -> f32 {
    if total_len == 0 {
        return 0.1;
    }
    let rel = position as f32 / total_len as f32;
    (1.0 - rel).max(0.1)
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
