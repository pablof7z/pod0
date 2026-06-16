//! Pure metadata-index backfill policy helpers.
//!
//! The kernel owns the decision of *which* episodes need metadata indexing
//! (and in what batch size / pacing). The backfill runs kernel-side via
//! `knowledge::spawn_metadata_index_backfill` — the Swift shell no longer
//! drives this process (the former `EpisodeMetadataIndexer` was retired in
//! slice 5f / #533 when the kernel took over as sole RAG owner).
//!
//! ## Doctrine
//!
//! * **D0 — Rust owns policy.** This module is the policy.
//! * **D6 — pure data in/out.** No I/O, no logging, no side effects.
//!
//! ## How it is used
//!
//! `metadata_index_backfill_candidates` is called by the kernel's internal
//! metadata-index backfill driver (`knowledge_search::spawn_metadata_index_backfill`).
//! It scans for un-indexed episodes, indexes their metadata chunks, and
//! dispatches `MarkEpisodesMetadataIndexed` internally.
//!
//! ## Ordering
//!
//! Episodes are returned in publication-date order (oldest-first within each
//! podcast, pods iterated by internal HashMap order — consistent within a
//! single run but not guaranteed across restarts).  A stable ordering is not
//! required for correctness.
//!
//! ## Why a flat batch instead of per-show batches
//!
//! Metadata indexing is a library-wide concern. Grouping by podcast adds no
//! correctness benefit and complicates the executor without improving the
//! user experience.

use super::PodcastStore;

/// Number of episode IDs surfaced per projection frame.
///
/// Chosen to match the value the Swift shell previously used (`batchSize = 32`).
/// Capped at 32 so a 5,000-episode cold-start backfill stays responsive and
/// respects rate limits on the embeddings provider.  Each batch completes before
/// the next frame is emitted (the shell dispatches `MarkEpisodesMetadataIndexed`
/// which bumps `Domain::Library`, triggering a new projection frame with the
/// next batch).
pub const METADATA_INDEX_BACKFILL_BATCH_SIZE: usize = 32;

/// Pause between backfill batches in milliseconds.
///
/// Used by the kernel-internal metadata-index backfill driver as the sleep
/// duration between successive embed calls. Chosen to match the former Swift
/// constant (`interBatchDelayNanoseconds = 200_000_000`, i.e. 200 ms) —
/// cheap insurance against rate-limiting the embeddings provider on a cold
/// launch with a large library.
pub const METADATA_INDEX_INTER_BATCH_DELAY_MS: u32 = 200;

impl PodcastStore {
    /// Return the next batch of episode IDs that need metadata indexing.
    ///
    /// Scans all known episodes (across every podcast), filters out those whose
    /// `metadata_indexed` flag is already set, and returns at most
    /// [`METADATA_INDEX_BACKFILL_BATCH_SIZE`] episode IDs (hyphenated UUID strings).
    ///
    /// **Returns an empty `Vec` when everything is already indexed** — the shell
    /// must stop its executor loop when the returned slice is empty.
    ///
    /// The scan is episode-level (not show-level) so one problematic podcast's
    /// episodes don't block the rest of the library.
    ///
    /// ## Ordering
    ///
    /// Episodes are iterated in the order stored per-podcast.  Within a show
    /// episodes are stored newest-first (RSS parser contract), so the returned
    /// batch is a mix of episodes from different shows — the shell may see a
    /// mix of old and new episodes across shows.  This is fine: the embed
    /// quality is not ordering-sensitive.
    pub fn metadata_index_backfill_candidates(&self) -> Vec<String> {
        let mut candidates = Vec::with_capacity(METADATA_INDEX_BACKFILL_BATCH_SIZE);
        'outer: for episodes in self.episodes.values() {
            for ep in episodes {
                let id_str = ep.id.0.to_string();
                if !self.metadata_indexed_episodes.contains(&id_str) {
                    candidates.push(id_str);
                    if candidates.len() >= METADATA_INDEX_BACKFILL_BATCH_SIZE {
                        break 'outer;
                    }
                }
            }
        }
        candidates
    }
}

#[cfg(test)]
#[path = "metadata_index_backfill_tests.rs"]
mod tests;
