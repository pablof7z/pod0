use serde::{Deserialize, Serialize};

use super::finite_f64_or_zero;

/// User-defined audio clip from an episode, surfaced via
/// [`super::snapshot::PodcastUpdate::clips`].
///
/// One row per saved clip. The kernel stores the clip internally as
/// `(id, episode_id, start, end, title, created_at)`; the snapshot
/// builder joins against `PodcastStore` to fill `episode_title` /
/// `podcast_title` at projection time so a podcast / episode rename
/// is reflected immediately without rewriting the clip record.
///
/// `start_secs` / `end_secs` are absolute positions inside the episode
/// (not relative). The kernel guarantees `start_secs < end_secs` at
/// create time.
///
/// `title` is the user-given clip title (e.g. "Marcus on retrieval"),
/// distinct from `episode_title`. `None` when the user did not name
/// the clip (e.g. AutoSnip with no follow-up rename).
///
/// `created_at` is Unix seconds — matches the timestamp convention
/// already used by `PendingApprovalSnapshot::requested_at`.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct ClipSummary {
    pub id: String,
    pub episode_id: String,
    pub episode_title: String,
    pub podcast_title: String,
    /// Clip start position in seconds, absolute within the episode.
    /// Non-finite values clamped to `0.0` at the wire boundary.
    #[serde(serialize_with = "finite_f64_or_zero")]
    pub start_secs: f64,
    /// Clip end position in seconds, absolute within the episode.
    /// Must satisfy `end_secs > start_secs` (enforced at create time).
    /// Non-finite values clamped to `0.0` at the wire boundary.
    #[serde(serialize_with = "finite_f64_or_zero")]
    pub end_secs: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Unix seconds when the clip was created. Set by the kernel
    /// (`chrono::Utc::now()` in the action handler) — never by the
    /// host.
    pub created_at: i64,
}
