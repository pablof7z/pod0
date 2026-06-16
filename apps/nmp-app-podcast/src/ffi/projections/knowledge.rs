use serde::{Deserialize, Serialize};

use super::finite_f32_or_zero;

/// One row in the RAG / vector-search projection surfaced via
/// [`super::snapshot::PodcastUpdate::knowledge_search_results`].
///
/// M6.A's `podcast-knowledge` crate owns the production chunk store +
/// hybrid ranker (KNN + BM25 + RRF + reranker). The iOS shell renders
/// from this narrow wire shape so the kernel can swap the underlying
/// implementation (current stub: case-insensitive substring match over
/// `Episode.title` + `Episode.description`; follow-up: real embedding
/// search) without breaking the host decoder.
///
/// Fields:
///
/// * `episode_id` / `episode_title` / `podcast_title` — what the row
///   labels itself with. `episode_id` is the hyphenated UUID so the
///   iOS shell can dispatch `podcast.player.play` against it.
/// * `snippet` — the relevant text excerpt (up to ~200 chars). The
///   projection layer truncates so the UI never has to.
/// * `start_secs` — position in the episode the snippet appears at,
///   when the underlying chunk has a timestamp (transcripts will;
///   description-only matches stay `None`). The iOS shell renders a
///   "seek to" button only when this is set.
/// * `relevance_score` — `0.0..=1.0`. Used by the UI to render a
///   relevance bar and (incidentally) to validate the ranker's order
///   in a regression test. The stub uses a simple "how early in the
///   text did the query land" heuristic.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct KnowledgeSearchResult {
    pub episode_id: String,
    pub episode_title: String,
    pub podcast_title: String,
    pub snippet: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_secs: Option<f64>,
    /// Relevance score `0.0..=1.0`.  Non-finite AI scores clamped to `0.0`.
    #[serde(serialize_with = "finite_f32_or_zero")]
    pub relevance_score: f32,
}


/// One row in the agent-memory projection surfaced via
/// [`super::snapshot::PodcastUpdate::memory_facts`].
///
/// Agent memory is a flat key→value bag the AI agent (and the user) can
/// write to so the assistant remembers durable facts about the user across
/// sessions (`"preferred_genre"` → `"technology"`,
/// `"timezone"` → `"Europe/Madrid"`, …). Keyed on `key` so writes upsert:
/// the most recent write wins.
///
/// `source` is `"user"` when the user wrote the fact through Settings,
/// `"agent"` when the assistant recorded it mid-conversation. Surfaced as
/// a string (not a typed enum) so the iOS decoder doesn't need a variant
/// case-mapping — matches every other `source` / `status` field on the
/// snapshot.
///
/// `created_at` is Unix seconds — the rendering layer formats it; the
/// projection stays format-free.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub struct MemoryFact {
    /// Stable id for the row. Currently the same as `key` so two writes
    /// from the same user collapse on upsert (key is the upsert handle).
    pub id: String,
    /// User-readable key (e.g. `"preferred_genre"`).
    pub key: String,
    /// Free-form value the agent or user wrote.
    pub value: String,
    /// `"user"` or `"agent"`. The action handler defaults missing values
    /// to `"user"` so the wire shape stays narrow for hand-written calls.
    pub source: String,
    /// Unix seconds when the fact was first written (preserved across
    /// upserts so the UI can show "remembered since …" if it wants to).
    pub created_at: i64,
}
