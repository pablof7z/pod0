//! Pure helper functions for the knowledge query FFI surface.

use std::collections::{HashMap, HashSet};
use std::ffi::CString;
use std::sync::{Arc, Mutex};

use podcast_knowledge::{cosine_similarity, KnowledgeStore, SearchResult};

use crate::ffi::actions::categorization_module::categorize_text;
use crate::llm::provider_transport::{EmbeddingIntent, ProviderKind};
use crate::store::PodcastStore;

use super::types::{HomeRelatedRow, KnowledgeQueryRow, QueryScope};

// ── Seed-query constructors ───────────────────────────────────────────────────

pub(super) fn similar_episode_seed_query(store: &PodcastStore, episode_id: &str) -> String {
    for (_podcast, episodes) in store.subscribed_podcasts() {
        for ep in episodes {
            if ep.id.0.to_string() == episode_id {
                let description_excerpt: String = ep.description.chars().take(400).collect();
                return [ep.title.clone(), description_excerpt]
                    .into_iter()
                    .filter(|part| !part.trim().is_empty())
                    .collect::<Vec<_>>()
                    .join(" ");
            }
        }
    }
    String::new()
}

pub(super) fn home_related_seed_query(store: &PodcastStore, episode_id: &str) -> String {
    for (_podcast, episodes) in store.subscribed_podcasts() {
        for ep in episodes {
            if ep.id.0.to_string() == episode_id {
                let mut parts = vec![ep.title.clone()];
                if let Some(chapters) = &ep.chapters {
                    parts.extend(
                        chapters
                            .iter()
                            .filter(|chapter| chapter.include_in_toc)
                            .map(|chapter| chapter.title.clone())
                            .filter(|title| !title.trim().is_empty())
                            .take(8),
                    );
                }
                if parts.len() == 1 {
                    let description_excerpt: String = ep.description.chars().take(400).collect();
                    parts.push(description_excerpt);
                }
                return parts
                    .into_iter()
                    .filter(|part| !part.trim().is_empty())
                    .collect::<Vec<_>>()
                    .join(" ");
            }
        }
    }
    String::new()
}

// ── Home-related projection helpers ──────────────────────────────────────────

pub(super) fn project_home_related_rows(
    rows: Vec<KnowledgeQueryRow>,
    seed_episode_id: &str,
    seed_podcast_id: Option<&str>,
    lens: &str,
    limit: usize,
) -> Vec<HomeRelatedRow> {
    let mut seen_podcasts = std::collections::HashSet::new();
    if lens == "topic" {
        if let Some(seed_podcast_id) = seed_podcast_id {
            seen_podcasts.insert(seed_podcast_id.to_owned());
        }
    }
    let mut out = Vec::new();
    for row in rows {
        if row.episode_id == seed_episode_id {
            continue;
        }
        if lens == "topic" && !seen_podcasts.insert(row.podcast_id.clone()) {
            continue;
        }
        out.push(HomeRelatedRow {
            id: format!("{}:{}:{}", row.episode_id, row.chunk_index, out.len()),
            episode_id: row.episode_id,
            podcast_id: row.podcast_id,
            episode_title: row.episode_title,
            podcast_title: row.podcast_title,
            chunk_index: row.chunk_index,
            text: row.text.chars().take(220).collect(),
        });
        if out.len() >= limit {
            break;
        }
    }
    out
}

pub(super) fn category_home_related_fallback(
    store: &PodcastStore,
    categories: &HashMap<String, Vec<String>>,
    seed_episode_id: &str,
    seed_podcast_id: Option<&str>,
    lens: &str,
    limit: usize,
) -> Vec<HomeRelatedRow> {
    let Some(seed_labels) = category_labels_for(store, categories, seed_episode_id) else {
        return Vec::new();
    };
    let seed_label_set: std::collections::HashSet<String> = seed_labels.into_iter().collect();
    let mut seen_podcasts = std::collections::HashSet::new();
    if lens == "topic" {
        if let Some(seed_podcast_id) = seed_podcast_id {
            seen_podcasts.insert(seed_podcast_id.to_owned());
        }
    }
    let mut out = Vec::new();

    for (podcast, episodes) in store.subscribed_podcasts() {
        let podcast_id = podcast.id.0.to_string();
        for ep in episodes {
            let episode_id = ep.id.0.to_string();
            if episode_id == seed_episode_id {
                seen_podcasts.insert(podcast_id.clone());
                continue;
            }
            let labels = category_labels_for(store, categories, &episode_id).unwrap_or_default();
            if !labels.iter().any(|label| seed_label_set.contains(label)) {
                continue;
            }
            if lens == "topic" && !seen_podcasts.insert(podcast_id.clone()) {
                continue;
            }
            out.push(HomeRelatedRow {
                id: format!("{episode_id}:category:{}", out.len()),
                episode_id,
                podcast_id: podcast_id.clone(),
                episode_title: ep.title.clone(),
                podcast_title: podcast.title.clone(),
                chunk_index: 0,
                text: fallback_snippet(store, &ep.id.0.to_string(), &ep.description),
            });
            if out.len() >= limit {
                return out;
            }
        }
    }
    out
}

pub(super) fn podcast_id_for_episode(store: &PodcastStore, episode_id: &str) -> Option<String> {
    for (podcast, episodes) in store.subscribed_podcasts() {
        if episodes.iter().any(|ep| ep.id.0.to_string() == episode_id) {
            return Some(podcast.id.0.to_string());
        }
    }
    None
}

fn category_labels_for(
    store: &PodcastStore,
    categories: &HashMap<String, Vec<String>>,
    episode_id: &str,
) -> Option<Vec<String>> {
    if let Some(labels) = categories.get(episode_id) {
        if !labels.is_empty() {
            return Some(labels.clone());
        }
    }
    for (_podcast, episodes) in store.subscribed_podcasts() {
        for ep in episodes {
            if ep.id.0.to_string() == episode_id {
                let labels = categorize_text(&ep.title, &ep.description);
                return (!labels.is_empty()).then_some(labels);
            }
        }
    }
    None
}

fn fallback_snippet(store: &PodcastStore, episode_id: &str, description: &str) -> String {
    let text = store.transcript_for(episode_id).unwrap_or(description);
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(220)
        .collect()
}

// ── Chunk / label helpers ─────────────────────────────────────────────────────

/// For a BM25-only result episode, pick the chunk that best matches the query
/// (highest BM25 score over that episode's transcript chunks) so the enriched
/// row's `text` / `chunk_index` / `start_secs` / `end_secs` reflect the matched
/// passage — not an arbitrary chunk 0.
///
/// Returns `None` when the episode has no indexed chunks (caller falls back to
/// the lean BM25 snippet — the episode matched on title/description only). When
/// the episode has chunks but none score against the query terms (the BM25
/// match came from title/description), returns the first chunk as a stable
/// best-effort anchor rather than nothing.
pub(super) fn best_matching_chunk(
    ks: &KnowledgeStore,
    episode_id: &str,
    query_terms: &[String],
) -> Option<podcast_knowledge::KnowledgeChunk> {
    let chunks = ks.chunks_for_episode(episode_id);
    if chunks.is_empty() {
        return None;
    }
    if query_terms.is_empty() {
        return chunks.into_iter().next();
    }
    // BM25 over this episode's chunks only — the top-ranked doc is the matched passage.
    let texts: Vec<&str> = chunks.iter().map(|c| c.chunk.text.as_str()).collect();
    let index = podcast_knowledge::bm25::Bm25Index::from_texts(&texts);
    match index.rank(query_terms).into_iter().next() {
        Some((doc, _)) => chunks.into_iter().nth(doc),
        // No chunk matched the query terms (title/description-only match) —
        // fall back to the first chunk as a stable anchor.
        None => chunks.into_iter().next(),
    }
}

/// Build `episode_id → (podcast_id, podcast_title, episode_title)`.
///
/// Extension of `crate::knowledge::build_episode_labels_pub` that includes
/// `podcast_id` so the rich DTO can carry it without a second store scan.
pub(super) fn build_rich_labels(
    store: &PodcastStore,
) -> HashMap<String, (String, String, String)> {
    let mut map = HashMap::new();
    for (podcast, episodes) in store.subscribed_podcasts() {
        let pid = podcast.id.0.to_string();
        for ep in episodes {
            map.insert(
                ep.id.0.to_string(),
                (pid.clone(), podcast.title.clone(), ep.title.clone()),
            );
        }
    }
    map
}

/// Derive the in-scope episode set from a `QueryScope`.
///
/// - `episode_id` set → exactly that one episode.
/// - `podcast_id` set → all episodes whose `podcast_id` matches.
/// - Neither set → `None` (whole library).
pub(super) fn scope_set_from(
    scope: &QueryScope,
    labels: &HashMap<String, (String, String, String)>,
) -> Option<HashSet<String>> {
    if let Some(ref ep_id) = scope.episode_id {
        return Some(std::iter::once(ep_id.clone()).collect());
    }
    if let Some(ref podcast_id) = scope.podcast_id {
        let set: HashSet<String> = labels
            .iter()
            .filter(|(_, (pid, _, _))| pid == podcast_id)
            .map(|(ep_id, _)| ep_id.clone())
            .collect();
        return Some(set);
    }
    None
}

/// Project rich labels to the lean `(podcast_title, episode_title)` map that
/// [`crate::knowledge_fusion::fuse_rrf`] requires.
pub(super) fn lean_labels_from(
    rich: &HashMap<String, (String, String, String)>,
) -> HashMap<String, (String, String)> {
    rich.iter()
        .map(|(ep_id, (_, pt, et))| (ep_id.clone(), (pt.clone(), et.clone())))
        .collect()
}

/// Scope-filtered cosine top-K search.
///
/// Iterates only embedded chunks; when `scope` is `Some`, skips chunks whose
/// `episode_id` is not in the set. Avoids cloning the full store for scope
/// filtering — only `SearchResult.chunk` (text metadata, no embedding) is cloned.
pub(super) fn scoped_top_k_search(
    store: &KnowledgeStore,
    query_embedding: &[f32],
    scope: Option<&HashSet<String>>,
    k: usize,
) -> Vec<SearchResult> {
    if k == 0 || query_embedding.is_empty() {
        return Vec::new();
    }
    let mut scored: Vec<SearchResult> = store
        .embedded()
        .filter(|(kc, _)| scope.map_or(true, |s| s.contains(&kc.chunk.episode_id)))
        .map(|(kc, emb)| SearchResult {
            chunk: kc.chunk.clone(),
            score: cosine_similarity(emb.as_slice(), query_embedding),
        })
        .collect();
    scored.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    scored.truncate(k);
    scored
}

/// Embed a query string using the configured provider.
///
/// Mirrors the degrade policy in `crate::state::knowledge_search` — returns
/// `None` on unusable model, missing key, transport error, or dim mismatch.
/// Never panics; the caller degrades to BM25-only when `None` is returned.
pub(super) async fn embed_query_for_rag(
    store_arc: &Arc<Mutex<PodcastStore>>,
    query: &str,
) -> Option<Vec<f32>> {
    let (provider, model) = {
        let s = store_arc.lock().ok()?;
        let model_str = s.embeddings_model().to_owned();
        let prov = if model_str.contains('/') {
            ProviderKind::OpenRouter
        } else if model_str.ends_with(":cloud") {
            ProviderKind::Ollama
        } else {
            return None; // unusable model → degrade to BM25
        };
        (prov, model_str)
    };
    let intent = EmbeddingIntent {
        provider,
        model: model.clone(),
        input: vec![query.to_owned()],
        dimensions: Some(podcast_knowledge::EXPECTED_EMBEDDING_DIM),
    };
    match crate::llm::provider_transport::embed(Arc::clone(store_arc), intent).await {
        Ok(r) => match r.embeddings.into_iter().next() {
            Some(v) if v.len() == podcast_knowledge::EXPECTED_EMBEDDING_DIM => Some(v),
            _ => None,
        },
        Err(_) => None,
    }
}

/// Build a bare `KnowledgeQueryRow` from a lean BM25 result (no chunk lookup).
/// Used on the index-lock-failure error path.
pub(super) fn bare_row(
    lean: crate::ffi::projections::KnowledgeSearchResult,
    labels: &HashMap<String, (String, String, String)>,
) -> KnowledgeQueryRow {
    let (podcast_id, podcast_title, episode_title) =
        labels.get(&lean.episode_id).cloned().unwrap_or_default();
    KnowledgeQueryRow {
        episode_id: lean.episode_id,
        podcast_id,
        episode_title,
        podcast_title,
        chunk_index: 0,
        start_secs: lean.start_secs.unwrap_or(0.0),
        end_secs: 0.0,
        text: lean.snippet,
        relevance_score: lean.relevance_score,
    }
}

// ── JSON envelope helpers ─────────────────────────────────────────────────────

pub(super) fn ok_json(value: &serde_json::Value) -> CString {
    CString::new(value.to_string())
        .unwrap_or_else(|_| CString::new(r#"{"error":"encoding"}"#).unwrap())
}

pub(super) fn err_json(reason: &str) -> CString {
    let json = serde_json::json!({"error": reason}).to_string();
    CString::new(json).unwrap_or_else(|_| CString::new(r#"{"error":"encoding"}"#).unwrap())
}
