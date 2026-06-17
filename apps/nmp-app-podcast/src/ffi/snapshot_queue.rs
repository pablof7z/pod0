//! Queue-projection helper for [`super::snapshot::build_snapshot_payload`].
//!
//! Extracted into a sibling module so [`super::snapshot`] stays under the
//! 500-line ceiling once additional projections land. The single helper
//! cross-references the queued episode-id list against the freshly-built
//! library projection so the iOS list can render artwork + podcast title
//! per row without a second pull.

use super::projections::{EpisodeSummary, PodcastSummary};
use crate::queue::QueuedPlaybackItem;

/// Cross-reference queued episode ids against the freshly-built library
/// projection so each queue row carries the metadata the iOS list needs
/// (title, artwork, podcast title). Ids whose episode is no longer in the
/// library (e.g. the user unsubscribed after queuing) are silently dropped —
/// the queue itself still holds them, but the UI projection won't render
/// orphaned rows.
pub(super) fn resolve_queue_rows(
    items: &[QueuedPlaybackItem],
    library: &[PodcastSummary],
) -> Vec<EpisodeSummary> {
    items
        .iter()
        .filter_map(|item| {
            let id_lower = item.episode_id.to_lowercase();
            let mut row = library
                .iter()
                .flat_map(|p| p.episodes.iter())
                .find(|ep| ep.id == id_lower)
                .cloned()?;
            row.queue_start_secs = item.start_secs;
            row.queue_end_secs = item.end_secs;
            row.queue_slot_id = Some(item.slot_id.clone());
            Some(row)
        })
        .collect()
}

#[cfg(test)]
#[path = "snapshot_queue_tests.rs"]
mod tests;
