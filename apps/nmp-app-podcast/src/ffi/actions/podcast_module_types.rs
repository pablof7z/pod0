//! Helper types for podcast actions.

use serde::{Deserialize, Serialize};

pub fn default_true() -> bool {
    true
}

/// One chapter for an [`super::super::PodcastAction::AddEpisode`] op. `image_url` +
/// `source_episode_id` carry the parity fields the Swift TTS composer built on
/// `Episode.Chapter` (mid-play artwork swap + source-episode chip). They round
/// the kernel store, not just the wire, so the projected chapter is identical
/// to the pre-kernel build.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct EpisodeChapterArg {
    pub start_secs: f64,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_episode_id: Option<String>,
}

/// One row in a [`super::PodcastAction::SetEpisodeTriage`] batch.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct EpisodeTriagePatch {
    pub episode_id: String,
    /// `"inbox"` | `"archived"` | `"none"` (sentinel: clear).
    pub decision: String,
    #[serde(default)]
    pub is_hero: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rationale: Option<String>,
}
