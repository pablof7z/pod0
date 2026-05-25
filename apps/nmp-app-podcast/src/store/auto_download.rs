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
//! Episodes are matched against the previously-known set by `guid`,
//! **not** `EpisodeId`. The latter is a fresh `Uuid::new_v4()` on
//! every parse (see `Episode::new`), so id-based diffing would mark
//! every episode as "new" on every refresh. The accumulator
//! (`podcast-feeds::rss::accumulator`) explicitly notes that `guid`
//! is the load-bearing stable identity.

use std::collections::{HashMap, HashSet};

use podcast_core::{Episode, EpisodeId};

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
pub fn episodes_to_auto_download(
    fresh: &[Episode],
    existing_guids: &HashSet<String>,
    local_paths: &HashMap<EpisodeId, String>,
    auto_download_on: bool,
) -> Vec<(EpisodeId, String)> {
    if !auto_download_on {
        return Vec::new();
    }
    fresh
        .iter()
        .filter(|ep| !existing_guids.contains(&ep.guid))
        .filter(|ep| !local_paths.contains_key(&ep.id))
        .map(|ep| (ep.id, ep.enclosure_url.to_string()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use podcast_core::{Episode, PodcastId};
    use url::Url;
    use uuid::Uuid;

    fn make_episode(podcast_id: PodcastId, guid: &str, url: &str) -> Episode {
        Episode::new(
            podcast_id,
            guid,
            "Title",
            Url::parse(url).unwrap(),
            chrono::Utc::now(),
        )
    }

    #[test]
    fn auto_download_off_does_not_queue_download() {
        let pid = PodcastId::generate();
        let fresh = vec![make_episode(pid, "g1", "https://ex.com/a.mp3")];
        let existing: HashSet<String> = HashSet::new();
        let local: HashMap<EpisodeId, String> = HashMap::new();

        let out = episodes_to_auto_download(&fresh, &existing, &local, false);
        assert!(out.is_empty());
    }

    #[test]
    fn auto_download_on_queues_new_episodes() {
        let pid = PodcastId::generate();
        let ep_known = make_episode(pid, "known", "https://ex.com/known.mp3");
        let ep_new = make_episode(pid, "new", "https://ex.com/new.mp3");
        let fresh = vec![ep_known.clone(), ep_new.clone()];

        let mut existing = HashSet::new();
        existing.insert("known".to_string());

        let out = episodes_to_auto_download(&fresh, &existing, &HashMap::new(), true);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].0, ep_new.id);
        assert_eq!(out[0].1, "https://ex.com/new.mp3");
    }

    #[test]
    fn auto_download_skips_episodes_with_existing_local_path() {
        let pid = PodcastId::generate();
        let ep = make_episode(pid, "new", "https://ex.com/new.mp3");
        let fresh = vec![ep.clone()];

        let mut local = HashMap::new();
        local.insert(ep.id, "/tmp/already-here.mp3".to_string());

        // Even with auto-download on and the guid unknown, an existing
        // local_path means the file is on disk — don't re-queue.
        let out = episodes_to_auto_download(&fresh, &HashSet::new(), &local, true);
        assert!(out.is_empty());
    }

    #[test]
    fn auto_download_preserves_input_order() {
        let pid = PodcastId::generate();
        let ep1 = make_episode(pid, "g1", "https://ex.com/1.mp3");
        let ep2 = make_episode(pid, "g2", "https://ex.com/2.mp3");
        let ep3 = make_episode(pid, "g3", "https://ex.com/3.mp3");
        let fresh = vec![ep1.clone(), ep2.clone(), ep3.clone()];

        let out = episodes_to_auto_download(&fresh, &HashSet::new(), &HashMap::new(), true);
        assert_eq!(out.len(), 3);
        assert_eq!(out[0].0, ep1.id);
        assert_eq!(out[1].0, ep2.id);
        assert_eq!(out[2].0, ep3.id);
    }

    #[test]
    fn auto_download_with_empty_fresh_list_returns_empty() {
        let out = episodes_to_auto_download(&[], &HashSet::new(), &HashMap::new(), true);
        assert!(out.is_empty());
    }

    #[test]
    fn auto_download_off_ignores_other_inputs() {
        // Even when guids are unknown and no local_paths exist, the off
        // switch must win.
        let pid = PodcastId::generate();
        let ep = make_episode(pid, "g1", "https://ex.com/a.mp3");
        let out = episodes_to_auto_download(
            &[ep],
            &HashSet::new(),
            &HashMap::new(),
            false,
        );
        assert!(out.is_empty());
    }

    /// Use a stable v4 UUID derived from a deterministic seed so the
    /// "id collision" path is reproducible in CI. Skip if `Uuid::new_v4`
    /// is the only constructor exposed.
    #[test]
    fn auto_download_matches_local_paths_by_episode_id() {
        let pid = PodcastId::generate();
        let mut ep = make_episode(pid, "guid-stable", "https://ex.com/a.mp3");
        // Force a known id so the local_paths map can target it
        // directly. Mirrors how a Completed download report would
        // have stamped a path keyed by this id at some earlier point.
        ep.id = EpisodeId::new(Uuid::nil());

        let mut local = HashMap::new();
        local.insert(ep.id, "/var/mobile/Downloads/a.mp3".to_string());

        let out = episodes_to_auto_download(&[ep], &HashSet::new(), &local, true);
        assert!(out.is_empty());
    }
}
