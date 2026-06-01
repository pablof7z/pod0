//! Episode AI-summary accessors on [`PodcastStore`].
//!
//! `summary` is a persisted field on `podcast_core::Episode` (it survives feed
//! refreshes and app restarts, mirroring `triage_rationale` / `chapters`).
//! This module exposes the two operations the summarization pipeline needs:
//!
//! * [`PodcastStore::episode_summary_inputs`] — gather the LLM prompt inputs
//!   (`title`, `description`, cached `transcript`) for one episode, under a
//!   single short read lock, so the actual Ollama call runs lock-free.
//! * [`PodcastStore::set_episode_summary`] — stamp the LLM result onto the
//!   episode and persist. Mirrors [`super::PodcastStore::set_episode_chapters`].

use super::PodcastStore;

/// The inputs the LLM summarizer needs for one episode.
#[derive(Clone, Debug, PartialEq)]
pub struct EpisodeSummaryInputs {
    pub title: String,
    pub description: String,
    /// Cached raw-text transcript, when one has been fetched/stored. `None`
    /// falls the summarizer back to title + description.
    pub transcript: Option<String>,
}

impl PodcastStore {
    /// Collect the title, description, and cached transcript for `id_str`.
    ///
    /// Returns `None` when no episode matches the id. The transcript is cloned
    /// from the transient `transcripts` cache (the same source
    /// [`PodcastStore::transcript_for`] reads) so the caller can drop the store
    /// lock before the Ollama round-trip.
    pub fn episode_summary_inputs(&self, id_str: &str) -> Option<EpisodeSummaryInputs> {
        for episodes in self.episodes.values() {
            if let Some(ep) = episodes.iter().find(|e| e.id.0.to_string() == id_str) {
                return Some(EpisodeSummaryInputs {
                    title: ep.title.clone(),
                    description: ep.description.clone(),
                    transcript: self.transcript_for(id_str).map(str::to_owned),
                });
            }
        }
        None
    }

    /// Return the persisted AI summary for `id_str`, if one has been stamped.
    pub fn episode_summary(&self, id_str: &str) -> Option<&str> {
        for episodes in self.episodes.values() {
            if let Some(ep) = episodes.iter().find(|e| e.id.0.to_string() == id_str) {
                return ep.summary.as_deref();
            }
        }
        None
    }

    /// Stamp `summary` onto the episode identified by `id_str` and persist.
    ///
    /// An empty string clears the field (`None`). Returns `true` when an
    /// episode matched and was updated, `false` when the id is unknown.
    /// Mirrors [`PodcastStore::set_episode_chapters`].
    pub fn set_episode_summary(&mut self, id_str: &str, summary: Option<String>) -> bool {
        let cleaned = summary.and_then(|s| {
            let t = s.trim().to_owned();
            if t.is_empty() { None } else { Some(t) }
        });
        for episodes in self.episodes.values_mut() {
            if let Some(ep) = episodes.iter_mut().find(|e| e.id.0.to_string() == id_str) {
                ep.summary = cleaned;
                self.persist();
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
#[path = "summary_tests.rs"]
mod tests;
