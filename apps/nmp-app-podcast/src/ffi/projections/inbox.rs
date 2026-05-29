use serde::{Deserialize, Serialize};

/// One row in the AI-triaged inbox surfaced via
/// [`super::snapshot::PodcastUpdate::inbox`]. The kernel projection is built
/// by [`crate::inbox_handler::build_inbox`] from the unlistened-∖-dismissed
/// set; the score is normalised to `0.0..=1.0` (higher = more important).
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct InboxItem {
    pub episode_id: String,
    pub episode_title: String,
    pub podcast_id: String,
    pub podcast_title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artwork_url: Option<String>,
    /// Unix seconds (`Episode::pub_date.timestamp()`).
    pub published_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_secs: Option<f64>,
    /// `0.0..=1.0`; higher = more important.
    pub priority_score: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority_reason: Option<String>,
    /// Topic labels the agent's heuristic categorizer assigned to this
    /// episode. Empty until
    /// [`super::actions::CategorizationModule`](super::actions::categorization_module::CategorizationModule)
    /// runs (auto-triggered after every successful feed refresh).
    ///
    /// At most three entries, ordered by keyword-match strength (strongest
    /// first). Wire field is omitted when empty so the byte-compatible
    /// legacy stub is preserved for cold-start snapshots (D5 / D6).
    #[serde(default)]
    pub ai_categories: Vec<String>,
}
