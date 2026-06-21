//! DTO structs for the knowledge query FFI surface.

use serde::{Deserialize, Serialize};

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
pub(super) struct QueryScope {
    pub(super) podcast_id: Option<String>,
    pub(super) episode_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct KnowledgeQueryRequest {
    pub(super) query: String,
    #[serde(default)]
    pub(super) scope: QueryScope,
    pub(super) limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub(super) struct KnowledgeChunkRequest {
    pub(super) episode_id: String,
    pub(super) chunk_index: u32,
}

#[derive(Debug, Deserialize)]
pub(super) struct SimilarEpisodeRequest {
    pub(super) episode_id: String,
    pub(super) limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub(super) struct HomeRelatedRequest {
    pub(super) episode_id: String,
    #[serde(default)]
    pub(super) lens: String,
    pub(super) limit: Option<usize>,
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
