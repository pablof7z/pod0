//! Pure auto-download policy helpers.
//!
//! Lifted out of [`super::PodcastStore`] / [`crate::host_op_handler`] so
//! the decision of *which* freshly-discovered episodes should be queued
//! for download can be unit-tested without standing up an `NmpApp` /
//! FFI mock.
//!
//! ## Doctrine
//!
//! * **D0 — Rust owns policy.** This module is the policy. The iOS
//!   `DownloadCapability` only executes the `StartDownload` commands
//!   the handler emits from these results.
//! * **D6 — pure data in/out.** No I/O, no logging, no side effects.
//!
//! ## Identity
//!
//! Episodes are matched against the previously-known set by `guid` in the
//! first filter, with a belt-and-suspenders `EpisodeId` check in the second.
//! Both are now stable across refreshes: `Episode::new` derives the id via
//! UUIDv5 from `(feed_url, guid)`, and `guid` is the RSS-canonical stable key.
//! The accumulator (`podcast-feeds::rss::accumulator`) records all guids so
//! the first filter accurately reflects what the store already contains.

use std::collections::{HashMap, HashSet};

use podcast_core::{Episode, EpisodeId};

/// How many of a show's most-recent *undownloaded* episodes a backfill /
/// evaluate pass queues. The fresh-feed refresh path (`episodes_to_auto_download`)
/// handles genuinely-new arrivals unbounded; this bound applies only to the
/// catch-up scan that runs on cold start and when the user *enables*
/// auto-download on a show that already has a back catalog — without it,
/// flipping "All new" on a 500-episode feed would queue the entire archive.
/// Chosen as "keep the latest few episodes local" rather than the full
/// back catalog. See [`super::PodcastStore::auto_download_backfill_candidates`].
pub const AUTO_DOWNLOAD_BACKFILL_LIMIT: usize = 3;

/// Decide which freshly-parsed episodes deserve to be auto-queued for
/// download.
///
/// Inputs:
///
/// * `fresh` — the new episode list returned by the feed parser.
/// * `existing_guids` — `guid` set captured *before* the store
///   accepted the refresh (any guid in this set is by definition
///   known and is not a "new" episode).
/// * `local_paths` — currently-known on-disk paths keyed by
///   `EpisodeId`. Used as a belt-and-suspenders filter so a fresh
///   episode that somehow already has a local file recorded for it
///   isn't re-queued.
/// * `auto_download_on` — whether the user has enabled auto-download
///   for the owning podcast. Short-circuits to an empty Vec when
///   `false`.
///
/// Output: an ordered list of `(EpisodeId, enclosure_url)` pairs the
/// handler should dispatch as `DownloadCommand::StartDownload` (one per
/// command). Ordering mirrors the input `fresh` slice (newest-first
/// per the parser's contract).
///
/// * `wifi_only_on` — when `true`, only downloads when `is_on_wifi` is also
///   `true`. When `false`, downloads on any interface (cellular + Wi-Fi).
/// * `is_on_wifi` — current network-path state reported by
///   `nmp.network.capability`. Ignored when `wifi_only_on` is `false`.
/// Returns `(ready, deferred)` where:
/// - `ready` — episodes to dispatch for download immediately.
/// - `deferred` — episodes that would auto-download but are gated on Wi-Fi
///   while the device is currently on cellular. The caller must persist these
///   so they can be dispatched when `NetworkReport::ConnectivityChanged`
///   reports Wi-Fi restored; otherwise their guids become "existing" on the
///   next refresh and they are permanently skipped.
pub fn episodes_to_auto_download(
    fresh: &[Episode],
    existing_guids: &HashSet<String>,
    local_paths: &HashMap<EpisodeId, String>,
    auto_download_on: bool,
    wifi_only_on: bool,
    is_on_wifi: bool,
) -> (Vec<(EpisodeId, String)>, Vec<(EpisodeId, String)>) {
    if !auto_download_on {
        return (Vec::new(), Vec::new());
    }
    let candidates: Vec<(EpisodeId, String)> = fresh
        .iter()
        .filter(|ep| !existing_guids.contains(&ep.guid))
        .filter(|ep| !local_paths.contains_key(&ep.id))
        .map(|ep| (ep.id, ep.enclosure_url.to_string()))
        .collect();

    if wifi_only_on && !is_on_wifi {
        // Defer rather than discard: the caller persists these so they can be
        // dispatched when Wi-Fi is restored.
        return (Vec::new(), candidates);
    }
    (candidates, Vec::new())
}

impl super::PodcastStore {
    /// Scan the *current* library for episodes that auto-download policy says
    /// should be on disk but aren't — the catch-up counterpart to
    /// [`episodes_to_auto_download`], which only sees freshly-parsed feed
    /// episodes. Runs on cold start (the foreground `RefreshAll` is skipped on
    /// the first activation) and when the user enables auto-download on a show
    /// that already has episodes (the fresh-GUID filter would otherwise skip
    /// every existing episode, so flipping the toggle downloaded nothing).
    ///
    /// For each podcast with auto-download enabled, takes its most-recent
    /// `limit_per_show` episodes that have no recorded local file, splitting
    /// them into `(ready, deferred)` by the show's Wi-Fi-only policy and the
    /// current `is_on_wifi` state. Episodes already in flight are filtered by
    /// the caller's idempotent enqueue, so re-running this is safe.
    pub fn auto_download_backfill_candidates(
        &self,
        is_on_wifi: bool,
        limit_per_show: usize,
    ) -> (Vec<(EpisodeId, String)>, Vec<(EpisodeId, String)>) {
        let mut ready = Vec::new();
        let mut deferred = Vec::new();
        for (&podcast_id, episodes) in self.episodes.iter() {
            if !self.is_auto_download_enabled(podcast_id) {
                continue;
            }
            let wifi_only = self.wifi_only_for(podcast_id);
            let mut taken = 0usize;
            for ep in episodes.iter() {
                if taken >= limit_per_show {
                    break;
                }
                if self.local_paths().contains_key(&ep.id) {
                    continue;
                }
                taken += 1;
                let item = (ep.id, ep.enclosure_url.to_string());
                if wifi_only && !is_on_wifi {
                    deferred.push(item);
                } else {
                    ready.push(item);
                }
            }
        }
        (ready, deferred)
    }
}

#[cfg(test)]
#[path = "auto_download_tests.rs"]
mod tests;
