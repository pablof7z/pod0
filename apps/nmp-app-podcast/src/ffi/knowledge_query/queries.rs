//! Async inner query runners for the knowledge query FFI surface.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use podcast_knowledge::{KnowledgeStore, SearchResult};

use crate::store::PodcastStore;

use super::helpers::{
    bare_row, best_matching_chunk, build_rich_labels, category_home_related_fallback,
    embed_query_for_rag, home_related_seed_query, lean_labels_from, podcast_id_for_episode,
    project_home_related_rows, scope_set_from, scoped_top_k_search, similar_episode_seed_query,
};
use super::types::{
    HomeRelatedRequest, HomeRelatedRow, KnowledgeQueryRequest, KnowledgeQueryRow, QueryScope,
    SimilarEpisodeRequest,
};

// ── Core query logic (async) ──────────────────────────────────────────────────

/// Inner async body for `nmp_app_podcast_knowledge_query`.
///
/// Extracted so tests can `runtime.block_on(...)` it directly without
/// constructing a full `PodcastHandle`.
///
/// ## Algorithm
///
/// 1. Build rich labels (`episode_id → (podcast_id, podcast_title, episode_title)`).
/// 2. Derive the in-scope episode set from `QueryScope` (podcast_id / episode_id / all).
/// 3. BM25 over the library (`collect_knowledge_matches_n`, `over_k = limit*4`),
///    filtered to the scope.
/// 4. Optionally embed the query via the configured provider and run scoped
///    cosine top-K. Degrades silently to BM25-only on: unusable model,
///    missing key, transport error, or dimension mismatch.
/// 5. RRF-fuse (k=60) both lists via `knowledge_fusion::fuse_rrf`.
/// 6. Enrich each result with full chunk text / `chunk_index` / `end_secs`
///    (vector-hit chunk wins; falls back to first indexed chunk for BM25-only).
pub(super) async fn run_knowledge_query_inner(
    req: KnowledgeQueryRequest,
    store_arc: Arc<Mutex<PodcastStore>>,
    index_arc: Arc<Mutex<KnowledgeStore>>,
) -> Vec<KnowledgeQueryRow> {
    use crate::knowledge::{collect_knowledge_matches_n, KNOWLEDGE_SEARCH_TOP_K};

    let query = req.query.trim().to_owned();
    if query.is_empty() {
        return Vec::new();
    }
    let top_k = req.limit.unwrap_or(KNOWLEDGE_SEARCH_TOP_K);
    let over_k = top_k * 4;

    // ── Phase 1a: build labels + scope, collect title/description BM25 (store lock) ──
    let (rich_labels, scope_set, lean_labels, mut bm25_rows) = {
        let Ok(s) = store_arc.lock() else {
            return Vec::new();
        };
        let rich = build_rich_labels(&s);
        let scope = scope_set_from(&req.scope, &rich);
        let lean = lean_labels_from(&rich);
        let bm25 = collect_knowledge_matches_n(&s, &query, over_k);
        (rich, scope, lean, bm25)
    };

    // ── Phase 1b: merge transcript-chunk BM25 hits into the candidate pool ───
    // Mirrors the reactive `KnowledgeState::search` path: a query that matches
    // only transcript content (no title/description hit) must still surface the
    // episode. Without this, chunk-content queries return nothing on the common
    // no-embeddings degrade path. `merge_chunk_matches_pub` dedups by episode and
    // carries chunk `start_secs`; Phase 5 re-derives the precise matched chunk
    // for full text + chunk_index. Store lock released above before this index
    // lock (sequential, never nested — lock-order rule §6.2).
    if let Ok(ks) = index_arc.lock() {
        crate::knowledge::merge_chunk_matches_pub(&mut bm25_rows, &ks, &query, &lean_labels);
    }

    // Apply scope filter AFTER the chunk merge (merge adds episodes from the
    // whole store; scope restricts to the requested podcast/episode).
    if let Some(ref sc) = scope_set {
        bm25_rows.retain(|r| sc.contains(&r.episode_id));
    }
    bm25_rows.sort_by(|a, b| {
        b.relevance_score
            .partial_cmp(&a.relevance_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    bm25_rows.truncate(over_k);

    // ── Phase 2: embed query (async — no locks held) ─────────────────────────
    let maybe_qvec = embed_query_for_rag(&store_arc, &query).await;

    // ── Phase 3: scoped cosine top-K + best-chunk map (index lock) ──────────
    let (vector_hits, best_vector_chunk) = {
        let Ok(ks) = index_arc.lock() else {
            // Index-lock failure: return BM25 rows with bare metadata
            return bm25_rows
                .into_iter()
                .take(top_k)
                .map(|lean| bare_row(lean, &rich_labels))
                .collect();
        };
        let hits = match maybe_qvec {
            Some(ref qvec) => scoped_top_k_search(&ks, qvec, scope_set.as_ref(), over_k),
            None => Vec::new(), // degrade
        };
        let mut best: HashMap<String, SearchResult> = HashMap::new();
        for hit in &hits {
            best.entry(hit.chunk.episode_id.clone())
                .or_insert_with(|| hit.clone());
        }
        (hits, best)
    };

    // ── Phase 4: RRF fusion ──────────────────────────────────────────────────
    let fused =
        crate::knowledge_fusion::fuse_rrf(bm25_rows, vector_hits, &lean_labels, top_k, 60.0);

    // ── Phase 5: enrich to rich DTO ─────────────────────────────────────────
    // Tokenize the query once so BM25-only rows can be enriched with the chunk
    // that actually produced the match (not an arbitrary chunk 0). Lock the
    // index once for the whole enrichment pass rather than per-row.
    let query_terms = podcast_knowledge::bm25::tokenize(&query);
    let ks_guard = index_arc.lock().ok();
    fused
        .into_iter()
        .map(|lean| {
            let (podcast_id, podcast_title, episode_title) =
                rich_labels.get(&lean.episode_id).cloned().unwrap_or_default();

            // Vector-hit chunk carries real chunk_index/end_secs/full text — use
            // it directly. For a BM25-only episode (no vector hit), enrich with the
            // chunk that ACTUALLY matched the query (highest BM25 score over that
            // episode's chunks), so text/chunk_index/start_secs/end_secs reflect the
            // matched passage. Falls back to the lean BM25 snippet only when the
            // episode has no indexed transcript chunks (title/description-only match).
            let (chunk_index, start_secs, end_secs, text) =
                if let Some(hit) = best_vector_chunk.get(&lean.episode_id) {
                    (
                        hit.chunk.chunk_index,
                        hit.chunk.start_secs,
                        hit.chunk.end_secs,
                        hit.chunk.text.clone(),
                    )
                } else {
                    let matched = ks_guard
                        .as_ref()
                        .and_then(|ks| best_matching_chunk(ks, &lean.episode_id, &query_terms));
                    matched
                        .map(|c| {
                            (
                                c.chunk.chunk_index,
                                c.chunk.start_secs,
                                c.chunk.end_secs,
                                c.chunk.text.clone(),
                            )
                        })
                        .unwrap_or((0, lean.start_secs.unwrap_or(0.0), 0.0, lean.snippet.clone()))
                };

            KnowledgeQueryRow {
                episode_id: lean.episode_id,
                podcast_id,
                episode_title,
                podcast_title,
                chunk_index,
                start_secs,
                end_secs,
                text,
                relevance_score: lean.relevance_score,
            }
        })
        .collect()
}

pub(super) async fn run_similar_episode_inner(
    req: SimilarEpisodeRequest,
    store_arc: Arc<Mutex<PodcastStore>>,
    index_arc: Arc<Mutex<KnowledgeStore>>,
) -> Vec<KnowledgeQueryRow> {
    let seed_query = {
        let Ok(store) = store_arc.lock() else {
            return Vec::new();
        };
        similar_episode_seed_query(&store, &req.episode_id)
    };
    if seed_query.is_empty() {
        return Vec::new();
    }
    let limit = req.limit.unwrap_or(crate::knowledge::KNOWLEDGE_SEARCH_TOP_K);
    let search_req = KnowledgeQueryRequest {
        query: seed_query,
        scope: QueryScope::default(),
        limit: Some(limit.saturating_add(1).saturating_mul(4)),
    };
    run_knowledge_query_inner(search_req, store_arc, index_arc)
        .await
        .into_iter()
        .filter(|row| row.episode_id != req.episode_id)
        .take(limit)
        .collect()
}

pub(super) async fn run_home_related_inner(
    req: HomeRelatedRequest,
    store_arc: Arc<Mutex<PodcastStore>>,
    index_arc: Arc<Mutex<KnowledgeStore>>,
    categories: HashMap<String, Vec<String>>,
) -> Vec<HomeRelatedRow> {
    let lens = match req.lens.as_str() {
        "sources" => "sources",
        _ => "topic",
    };
    let limit = req.limit.unwrap_or(if lens == "sources" { 24 } else { 8 });
    let (query, seed_podcast_id) = {
        let Ok(store) = store_arc.lock() else {
            return Vec::new();
        };
        (
            home_related_seed_query(&store, &req.episode_id),
            podcast_id_for_episode(&store, &req.episode_id),
        )
    };
    if query.is_empty() {
        return Vec::new();
    }

    let search_limit = if lens == "sources" { limit } else { limit.saturating_mul(6) };
    let search_req = KnowledgeQueryRequest {
        query,
        scope: QueryScope::default(),
        limit: Some(search_limit),
    };
    let rows = run_knowledge_query_inner(search_req, Arc::clone(&store_arc), index_arc).await;
    let projected = project_home_related_rows(
        rows,
        &req.episode_id,
        seed_podcast_id.as_deref(),
        lens,
        limit,
    );
    if !projected.is_empty() {
        return projected;
    }

    let Ok(store) = store_arc.lock() else {
        return Vec::new();
    };
    category_home_related_fallback(
        &store,
        &categories,
        &req.episode_id,
        seed_podcast_id.as_deref(),
        lens,
        limit,
    )
}
