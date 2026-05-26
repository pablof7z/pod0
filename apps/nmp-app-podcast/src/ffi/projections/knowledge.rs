use serde::{Deserialize, Serialize};

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
    pub relevance_score: f32,
}

/// One row in [`super::snapshot::PodcastUpdate::wiki_articles`].
///
/// A `WikiArticle` is the AI-synthesised, per-podcast knowledge entry the user
/// builds up over time. Each article is keyed by `id` (UUID) and scoped to a
/// single `podcast_id`; `topic` is the user-supplied subject line and
/// `summary` is the LLM-rendered body (1-2 paragraphs in the scaffold; real
/// synthesis is a follow-up).
///
/// `source_episode_ids` lists the episode ids the synthesis drew from, so the
/// iOS reader can render tappable provenance rows that jump to the relevant
/// episode detail screen. `last_updated_at` is unix seconds — Swift can
/// compare against `Date()` without a formatter round-trip, mirroring the
/// pattern used by [`PendingApprovalSnapshot::requested_at`].
///
/// `is_generating` is the lifecycle flag the UI flips on while a generation
/// is in flight. In the scaffold the action handler completes synchronously
/// (`is_generating = false`); the field exists so the LLM-backed follow-up
/// can mutate it without renegotiating the wire shape.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub struct WikiArticle {
    /// Stable UUID for the article (hyphenated). The iOS reader uses this
    /// as the `Identifiable.id` and as the argument to
    /// `podcast.wiki.delete`.
    pub id: String,
    /// Owning podcast id (matches [`PodcastSummary::id`]). Used to filter
    /// the article list down to the current show on the iOS side.
    pub podcast_id: String,
    /// User-supplied subject — what the article is *about*.
    pub topic: String,
    /// Rendered body (1-2 paragraph summary in the scaffold).
    pub summary: String,
    /// Episode ids the synthesis drew from. Empty in the scaffold —
    /// populated once the LLM follow-up wires real retrieval.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_episode_ids: Vec<String>,
    /// Unix seconds — see struct-level comment.
    pub last_updated_at: i64,
    /// `true` while a generation is in flight; `false` once the article is
    /// readable. Lets the UI render a progress indicator without polling.
    pub is_generating: bool,
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
