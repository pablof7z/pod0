//! `nmp_app_podcast_knowledge_query` / `nmp_app_podcast_knowledge_similar_episode`
//! / `nmp_app_podcast_knowledge_chunk` — synchronous kernel RAG query surface
//! (slice 5b).
//!
//! `block_on` FFI functions returning inline JSON. Distinct from the
//! reactive `KnowledgeAction::Search` which stages results into the
//! `PodcastUpdate` projection — these are request/response (synchronous, no
//! domain bump, no store mutation) intended for agent (5d) and wiki (5e).
//!
//! ## Threading
//!
//! Both FFIs BLOCK the calling thread via `block_on`. Call from a Swift
//! `async` detached Task or background thread — NEVER the kernel actor thread.

use std::collections::{HashMap, HashSet};
use std::ffi::{c_char, CStr, CString};
use std::sync::{Arc, Mutex};

use podcast_knowledge::{cosine_similarity, KnowledgeStore, SearchResult};
use serde::{Deserialize, Serialize};

use crate::ffi::guard::ffi_guard;
use crate::ffi::handle::PodcastHandle;
use crate::llm::provider_transport::{EmbeddingIntent, ProviderKind};
use crate::store::PodcastStore;

// ── Rich result DTO ───────────────────────────────────────────────────────────

/// Chunk-level RAG query result row returned by `nmp_app_podcast_knowledge_query`.
///
/// Distinct from the lean [`crate::ffi::projections::KnowledgeSearchResult`]
/// used by the Search-tab projection (per-episode, 200-char snippet). This DTO
/// carries:
/// * Full chunk `text` (no truncation — LLM callers need real context).
/// * `chunk_index` / `end_secs` — exact chunk boundaries for context windowing.
/// * `podcast_id` — so callers can scope follow-up queries without a library scan.
///
/// Serialised with default `serde::Serialize` → snake_case JSON field names.
/// Swift 5d/5e decoders will use `convertFromSnakeCase`; no explicit
/// `CodingKeys` annotation is needed on the Swift side.
#[derive(Debug, Serialize)]
pub struct KnowledgeQueryRow {
    pub episode_id: String,
    pub podcast_id: String,
    pub episode_title: String,
    pub podcast_title: String,
    pub chunk_index: u32,
    pub start_secs: f64,
    pub end_secs: f64,
    /// Full chunk text (no 200-char cap — agent / wiki callers need real LLM context).
    pub text: String,
    /// RRF-fused relevance score or BM25 score on the degrade path.
    pub relevance_score: f32,
}

// ── Input DTOs ────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Default)]
struct QueryScope {
    podcast_id: Option<String>,
    episode_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct KnowledgeQueryRequest {
    query: String,
    #[serde(default)]
    scope: QueryScope,
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct KnowledgeChunkRequest {
    episode_id: String,
    chunk_index: u32,
}

#[derive(Debug, Deserialize)]
struct SimilarEpisodeRequest {
    episode_id: String,
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct HomeRelatedRequest {
    episode_id: String,
    #[serde(default)]
    lens: String,
    limit: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct HomeRelatedRow {
    pub id: String,
    pub episode_id: String,
    pub podcast_id: String,
    pub episode_title: String,
    pub podcast_title: String,
    pub chunk_index: u32,
    pub text: String,
}

// ── FFI entry points ──────────────────────────────────────────────────────────

/// Synchronous hybrid RAG query: BM25 + optional semantic + RRF fusion.
///
/// # Request JSON
///
/// ```json
/// {"query":"…","scope":{"podcast_id":"…"},"limit":10}
/// ```
///
/// Scope (`scope` field or fields within):
/// - `{"podcast_id":"…"}` — restrict to that podcast's episodes only.
/// - `{"episode_id":"…"}` — restrict to that episode's chunks only.
/// - absent / `{}` — whole library.
///
/// `limit` defaults to 10.
///
/// # Response JSON
///
/// ```json
/// {"result":[{"episode_id":"…","podcast_id":"…","episode_title":"…",
///             "podcast_title":"…","chunk_index":0,"start_secs":0.0,
///             "end_secs":30.0,"text":"…","relevance_score":0.85},...]}
/// ```
///
/// or `{"error":"…"}` on hard failure.
#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_podcast_knowledge_query(
    handle: *mut PodcastHandle,
    request_json: *const c_char,
) -> *mut c_char {
    if handle.is_null() || request_json.is_null() {
        return err_json("null argument").into_raw();
    }
    ffi_guard(
        "nmp_app_podcast_knowledge_query",
        || err_json("panic").into_raw(),
        || {
            let json_str = match unsafe { CStr::from_ptr(request_json) }.to_str() {
                Ok(s) => s,
                Err(_) => return err_json("invalid UTF-8").into_raw(),
            };
            let req: KnowledgeQueryRequest = match serde_json::from_str(json_str) {
                Ok(r) => r,
                Err(e) => return err_json(&format!("JSON parse: {e}")).into_raw(),
            };
            let h = unsafe { &*handle };
            let store_arc = Arc::clone(&h.state.library.store);
            let index_arc = h.state.knowledge.index_arc();
            let runtime = Arc::clone(&h.state.infra.runtime);
            let rows = runtime.block_on(run_knowledge_query_inner(req, store_arc, index_arc));
            ok_json(&serde_json::json!({ "result": rows })).into_raw()
        },
    )
}

/// Synchronous similar-episode lookup.
///
/// Rust owns the seed-query policy: resolve the seed episode from the store,
/// derive search text from title + description excerpt, run the shared hybrid
/// RAG path, and remove the seed episode from returned rows.
///
/// # Request JSON
///
/// ```json
/// {"episode_id":"…","limit":10}
/// ```
#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_podcast_knowledge_similar_episode(
    handle: *mut PodcastHandle,
    request_json: *const c_char,
) -> *mut c_char {
    if handle.is_null() || request_json.is_null() {
        return err_json("null argument").into_raw();
    }
    ffi_guard(
        "nmp_app_podcast_knowledge_similar_episode",
        || err_json("panic").into_raw(),
        || {
            let json_str = match unsafe { CStr::from_ptr(request_json) }.to_str() {
                Ok(s) => s,
                Err(_) => return err_json("invalid UTF-8").into_raw(),
            };
            let req: SimilarEpisodeRequest = match serde_json::from_str(json_str) {
                Ok(r) => r,
                Err(e) => return err_json(&format!("JSON parse: {e}")).into_raw(),
            };
            let h = unsafe { &*handle };
            let store_arc = Arc::clone(&h.state.library.store);
            let index_arc = h.state.knowledge.index_arc();
            let runtime = Arc::clone(&h.state.infra.runtime);
            let rows = runtime.block_on(run_similar_episode_inner(req, store_arc, index_arc));
            ok_json(&serde_json::json!({ "result": rows })).into_raw()
        },
    )
}

/// Home's "Related" sheet projection.
///
/// Rust owns the product policy: seed-query construction, topic-vs-source
/// lens limits, seed filtering, one-row-per-show collapse for the topic lens,
/// and the category fallback when the transcript index is empty.
#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_podcast_knowledge_home_related(
    handle: *mut PodcastHandle,
    request_json: *const c_char,
) -> *mut c_char {
    if handle.is_null() || request_json.is_null() {
        return err_json("null argument").into_raw();
    }
    ffi_guard(
        "nmp_app_podcast_knowledge_home_related",
        || err_json("panic").into_raw(),
        || {
            let json_str = match unsafe { CStr::from_ptr(request_json) }.to_str() {
                Ok(s) => s,
                Err(_) => return err_json("invalid UTF-8").into_raw(),
            };
            let req: HomeRelatedRequest = match serde_json::from_str(json_str) {
                Ok(r) => r,
                Err(e) => return err_json(&format!("JSON parse: {e}")).into_raw(),
            };
            let h = unsafe { &*handle };
            let store_arc = Arc::clone(&h.state.library.store);
            let index_arc = h.state.knowledge.index_arc();
            let runtime = Arc::clone(&h.state.infra.runtime);
            let categories = h.state.categories.categories_snapshot();
            let rows = runtime.block_on(run_home_related_inner(
                req, store_arc, index_arc, categories,
            ));
            ok_json(&serde_json::json!({ "result": rows })).into_raw()
        },
    )
}

/// Synchronous chunk lookup by `(episode_id, chunk_index)`.
///
/// # Request JSON
///
/// ```json
/// {"episode_id":"…","chunk_index":0}
/// ```
///
/// # Response JSON
///
/// `{"result":{…}}` — same shape as one `KnowledgeQueryRow`.
/// `{"result":null}` when the chunk is not in the store.
///
/// # Threading
///
/// Pure in-memory lookup (no network). Still call from a non-actor thread.
#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_podcast_knowledge_chunk(
    handle: *mut PodcastHandle,
    request_json: *const c_char,
) -> *mut c_char {
    if handle.is_null() || request_json.is_null() {
        return err_json("null argument").into_raw();
    }
    ffi_guard(
        "nmp_app_podcast_knowledge_chunk",
        || err_json("panic").into_raw(),
        || {
            let json_str = match unsafe { CStr::from_ptr(request_json) }.to_str() {
                Ok(s) => s,
                Err(_) => return err_json("invalid UTF-8").into_raw(),
            };
            let req: KnowledgeChunkRequest = match serde_json::from_str(json_str) {
                Ok(r) => r,
                Err(e) => return err_json(&format!("JSON parse: {e}")).into_raw(),
            };
            let h = unsafe { &*handle };
            let store_arc = Arc::clone(&h.state.library.store);
            let index_arc = h.state.knowledge.index_arc();

            let labels = match store_arc.lock() {
                Ok(s) => build_rich_labels(&s),
                Err(_) => return err_json("store poisoned").into_raw(),
            };

            let row: Option<KnowledgeQueryRow> = match index_arc.lock() {
                Ok(ks) => ks
                    .chunks
                    .iter()
                    .find(|c| {
                        c.chunk.episode_id == req.episode_id
                            && c.chunk.chunk_index == req.chunk_index
                    })
                    .map(|kc| {
                        let (podcast_id, podcast_title, episode_title) = labels
                            .get(&kc.chunk.episode_id)
                            .cloned()
                            .unwrap_or_default();
                        KnowledgeQueryRow {
                            episode_id: kc.chunk.episode_id.clone(),
                            podcast_id,
                            episode_title,
                            podcast_title,
                            chunk_index: kc.chunk.chunk_index,
                            start_secs: kc.chunk.start_secs,
                            end_secs: kc.chunk.end_secs,
                            text: kc.chunk.text.clone(),
                            relevance_score: 0.0,
                        }
                    }),
                Err(_) => return err_json("knowledge_store poisoned").into_raw(),
            };

            ok_json(&serde_json::json!({ "result": row })).into_raw()
        },
    )
}

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
async fn run_knowledge_query_inner(
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

async fn run_similar_episode_inner(
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

async fn run_home_related_inner(
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

// ── Pure helpers ──────────────────────────────────────────────────────────────

fn similar_episode_seed_query(store: &PodcastStore, episode_id: &str) -> String {
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

fn home_related_seed_query(store: &PodcastStore, episode_id: &str) -> String {
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

fn project_home_related_rows(
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

fn category_home_related_fallback(
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

fn podcast_id_for_episode(store: &PodcastStore, episode_id: &str) -> Option<String> {
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
pub(crate) fn best_matching_chunk(
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
pub(crate) fn build_rich_labels(
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
fn scope_set_from(
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
fn lean_labels_from(
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
pub(crate) fn scoped_top_k_search(
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
async fn embed_query_for_rag(
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
fn bare_row(
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

fn ok_json(value: &serde_json::Value) -> CString {
    CString::new(value.to_string())
        .unwrap_or_else(|_| CString::new(r#"{"error":"encoding"}"#).unwrap())
}

fn err_json(reason: &str) -> CString {
    let json = serde_json::json!({"error": reason}).to_string();
    CString::new(json).unwrap_or_else(|_| CString::new(r#"{"error":"encoding"}"#).unwrap())
}

#[cfg(test)]
#[path = "knowledge_query_tests.rs"]
mod tests;
