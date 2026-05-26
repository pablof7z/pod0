//! RAG / knowledge-search ranker — stub backing for the
//! `podcast.knowledge.*` action namespace.
//!
//! M6.A's `podcast-knowledge` crate owns the production chunk store +
//! hybrid ranker (KNN + BM25 + RRF + reranker). Feature #38 ships the
//! iOS UI + wire contract while that pipeline is being wired up; this
//! module is the placeholder ranker that lets the UI work today.
//!
//! Lives in its own module (rather than inline in
//! [`crate::host_op_handler`]) so:
//!
//! * `host_op_handler.rs` stays under the project's 500-line hard limit.
//! * The ranker can be unit-tested directly against a `PodcastStore`
//!   without constructing an `NmpApp`.
//! * Swapping the stub for the real `podcast-knowledge` ranker is a
//!   one-function replacement.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crate::ffi::actions::knowledge_module::KnowledgeAction;
use crate::ffi::projections::KnowledgeSearchResult;
use crate::store::PodcastStore;

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
    rev: &Arc<AtomicU64>,
) -> serde_json::Value {
    match action {
        KnowledgeAction::Search { query } => handle_search(query, store, slot, rev),
        KnowledgeAction::ClearResults => handle_clear_results(slot, rev),
        KnowledgeAction::IndexEpisode { episode_id: _ } => {
            // Stub for feature #38. Real ingestion (chunking, embedding,
            // upsert into the `podcast-knowledge` chunk store) lands in
            // M6.B alongside the transcript pipeline.
            serde_json::json!({"ok": true, "status": "indexed"})
        }
    }
}

fn handle_search(
    query: String,
    store: &Arc<Mutex<PodcastStore>>,
    slot: &Arc<Mutex<Vec<KnowledgeSearchResult>>>,
    rev: &Arc<AtomicU64>,
) -> serde_json::Value {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        // Empty query clears the slot — same semantics as
        // `clear_results` so the UI doesn't have to special-case.
        return handle_clear_results(slot, rev);
    }
    let rows = match store.lock() {
        Ok(s) => collect_knowledge_matches(&s, trimmed),
        Err(_) => return serde_json::json!({"ok": false, "error": "store poisoned"}),
    };
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

/// Stub RAG ranker: case-insensitive substring match over each
/// episode's title + description, sorted by [`score_match`] and
/// truncated to [`KNOWLEDGE_SEARCH_TOP_K`].
///
/// M6.B will replace the body with a hybrid KNN + BM25 ranker pulling
/// from the `podcast-knowledge` chunk store; the function signature
/// (read-only `&PodcastStore`, owned `Vec<KnowledgeSearchResult>`)
/// stays stable so the call-site in `host_op_handler` does not change.
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
