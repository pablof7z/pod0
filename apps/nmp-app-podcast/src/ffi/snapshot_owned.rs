//! NIP-F4 owned-podcasts snapshot helper (features #27/#28).
//!
//! Joins [`crate::store::PodcastKeyStore`] (which holds the per-podcast
//! pubkeys) with [`crate::ffi::handle::OwnedPublishState`] (which
//! retains the last-built show event JSON + last-published timestamp)
//! and emits one [`OwnedPodcastInfo`] row per owned podcast.
//!
//! Lives in a sibling file to keep [`super::snapshot`] under the
//! 500-LOC hard limit (AGENTS.md).

use super::handle::PodcastHandle;
use super::projections::OwnedPodcastInfo;

/// Build the `owned_podcasts` projection for one snapshot tick.
///
/// Returns an empty vec when nothing is owned. Failures on the
/// individual `Mutex::lock()` calls degrade silently (D6) — a poisoned
/// `podcast_keys` map just means we surface no owned podcasts that
/// tick, not a kernel-wide failure.
pub fn collect_owned_podcasts(handle: &PodcastHandle) -> Vec<OwnedPodcastInfo> {
    // Step 13: podcast_keys and publish_state now in state.publish (PublishState).
    let pairs = handle
        .state
        .publish
        .podcast_keys
        .lock()
        .ok()
        .map(|k| k.iter_pubkeys())
        .unwrap_or_default();
    let state_map = handle.state.publish.publish_state.lock().ok();
    pairs
        .into_iter()
        .map(|(podcast_id, podcast_pubkey_hex)| {
            let (show_event_json, last_published_at) = state_map
                .as_ref()
                .and_then(|m| m.get(&podcast_id).cloned())
                .map(|s| (s.show_event_json, s.last_published_at))
                .unwrap_or_default();
            OwnedPodcastInfo {
                podcast_id,
                podcast_pubkey_hex,
                show_event_json,
                last_published_at,
            }
        })
        .collect()
}
