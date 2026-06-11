//! Builds the [`WidgetSnapshot`] platform projection.
//!
//! This is the kernel-side home of the home-screen widget's state. It used to
//! be derived natively on the iOS side (a parallel `NowPlayingSnapshot` the
//! Swift layer assembled from playback callbacks + a library lookup); D4
//! (one source of truth) requires the kernel to own the shape and the
//! derivation so there is a single canonical widget path.
//!
//! Built from the player projection (`now_playing: Option<&PlayerState>`) and
//! the already-assembled `library: &[PodcastSummary]` (see
//! [`super::snapshot::build_podcast_update`]) so it reuses the resolved
//! per-show `unplayed_count` without taking a second store lock or rescanning
//! every episode. The only per-tick work is a single linear walk over the
//! library to resolve the active episode's title/show/artwork — the same walk
//! the iOS `applyNowPlayingSnapshot` performed, now done once kernel-side.

use super::projections::{PodcastSummary, WidgetSnapshot};
use crate::player::PlayerState;

/// Build the widget projection from the current player state and the assembled
/// library.
///
/// Returns `None` when no episode is loaded *and* the library has no unplayed
/// episodes — there is nothing for the widget to show, so the kernel emits a
/// `null` widget and the host clears the App Group key (the widget renders its
/// empty state). When either an episode is playing *or* there are unplayed
/// episodes to badge, a `Some` is returned.
///
/// `position_fraction` is clamped to `0.0..=1.0` and is `0.0` whenever the
/// duration is zero/unknown (avoids a divide-by-zero ring on a freshly-loaded
/// episode). `position_secs`/`duration_secs` are carried verbatim so the
/// widget can render its exact "−MM:SS remaining" label.
///
/// `unplayed_count` sums the per-show `unplayed_count` the library projection
/// already computed, across **subscribed** shows only — known-but-unfollowed
/// feeds (ingested for external listing/playback) don't contribute to the
/// "to listen" badge.
pub fn build_widget_snapshot(
    now_playing: Option<&PlayerState>,
    library: &[PodcastSummary],
) -> Option<WidgetSnapshot> {
    let unplayed_count: usize = library
        .iter()
        .filter(|p| p.is_subscribed)
        .map(|p| p.unplayed_count)
        .sum();

    // Resolve the active episode's display strings from the library, mirroring
    // the old iOS `applyNowPlayingSnapshot` lookup: episode-level artwork wins,
    // falling back to the show artwork.
    let mut episode_title: Option<String> = None;
    let mut podcast_title: Option<String> = None;
    let mut artwork_url: Option<String> = None;
    let mut is_playing = false;
    let mut position_secs = 0.0;
    let mut duration_secs = 0.0;
    let mut position_fraction = 0.0_f32;
    let mut chapter_title: Option<String> = None;

    let loaded = now_playing.filter(|state| state.episode_id.is_some());

    if let Some(state) = loaded {
        is_playing = state.is_playing;
        position_secs = state.position_secs;
        duration_secs = state.duration_secs;
        chapter_title = state.current_chapter_title.clone();

        let episode_id = state.episode_id.as_deref();
        if let Some(episode_id) = episode_id {
            'outer: for podcast in library {
                for episode in &podcast.episodes {
                    if episode.id == episode_id {
                        episode_title = Some(episode.title.clone());
                        podcast_title = Some(podcast.title.clone());
                        artwork_url = episode
                            .artwork_url
                            .clone()
                            .or_else(|| podcast.artwork_url.clone());
                        // Prefer the library/feed-metadata duration over the
                        // player's: `PlayerState::duration_secs` stays 0.0
                        // until the audio capability emits its first Playing
                        // report, but the feed duration is known the moment
                        // the episode row exists. This mirrors the old iOS
                        // `applyNowPlayingSnapshot` preference so the widget's
                        // remaining-time label is correct before playback
                        // engages.
                        if duration_secs <= 0.0 {
                            if let Some(feed) = episode.duration_secs {
                                if feed > 0.0 {
                                    duration_secs = feed;
                                }
                            }
                        }
                        break 'outer;
                    }
                }
            }
        }
        // Fall back to the raw episode id as a title if the library lookup
        // missed (e.g. a still-streaming external episode not in the followed
        // library) so the widget never renders a blank face while playing.
        if episode_title.is_none() {
            episode_title = episode_id.map(str::to_owned);
        }
        position_fraction = clamp_fraction(position_secs, duration_secs);
    }

    // Nothing to show: no episode loaded and nothing unplayed to badge.
    if loaded.is_none() && unplayed_count == 0 {
        return None;
    }

    Some(WidgetSnapshot {
        now_playing_episode_title: episode_title,
        now_playing_podcast_title: podcast_title,
        now_playing_artwork_url: artwork_url,
        now_playing_chapter_title: chapter_title,
        is_playing,
        position_fraction,
        position_secs,
        duration_secs,
        unplayed_count,
    })
}

/// Clamp `position / duration` to `0.0..=1.0`. Returns `0.0` for a zero or
/// non-finite duration (freshly-loaded episode, capability hasn't reported a
/// duration yet) so the widget never divides by zero or renders a NaN ring.
fn clamp_fraction(position_secs: f64, duration_secs: f64) -> f32 {
    if duration_secs <= 0.0 || !duration_secs.is_finite() {
        return 0.0;
    }
    let fraction = (position_secs / duration_secs).clamp(0.0, 1.0);
    fraction as f32
}

#[cfg(test)]
#[path = "snapshot_widget_tests.rs"]
mod tests;
