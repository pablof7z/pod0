//! Slice-local store helpers for the domain payload builders.
//!
//! These helpers read directly from the `PodcastStore` without going through the
//! full `build_podcast_update` / `build_library_snapshot` pipeline.  They are
//! used by the per-domain builders in `snapshot_domain_builders.rs` to satisfy
//! their narrow data needs without triggering a full library rebuild.
//!
//! Both helpers are `pub(super)` — they are only called from the sibling
//! `snapshot_domain_builders` module (also `pub(super)` within `ffi`).

use std::collections::HashMap;

use super::handle::PodcastHandle;
use super::projections::TranscriptEntry;
use crate::queue::QueuedPlaybackItem;

/// Build `EpisodeSummary` rows for the given queued episode IDs by reading the
/// store directly — slice-local over ONLY the queued episodes, never a full
/// library rebuild over all episodes.
///
/// This is the slice-local equivalent of `resolve_queue_rows(ids, &library)`.
/// Each matched row is constructed via the SHARED [`episode_summary`] helper —
/// the exact same function `build_library_snapshot` uses per episode — so each
/// queue row is BYTE-IDENTICAL to the row the full-library path would emit for
/// the same episode: same `clean_html` description, same transcript / chapters /
/// ai_categories / ad_segments / triage / transcript_status, and the same
/// LOWERCASE `ep.id.0.to_string()` id.
///
/// IDs not found in the store (e.g. the user unsubscribed after queuing) are
/// silently dropped — matching the existing `resolve_queue_rows` behaviour.
///
/// `transcripts` and `categories_cache` are the same pre-snapshotted caches the
/// library path uses (keyed by lowercase episode id); the caller
/// (`build_playback_payload`) threads them in so the queue rows resolve
/// transcript entries + AI categories identically to the library rows.
///
/// [`episode_summary`]: super::snapshot_library::episode_summary
pub(super) fn build_queue_rows_from_store(
    handle: &PodcastHandle,
    items: &[QueuedPlaybackItem],
    transcripts: &HashMap<String, Vec<TranscriptEntry>>,
    categories_cache: &HashMap<String, Vec<String>>,
) -> Vec<super::projections::EpisodeSummary> {
    use super::snapshot_library::episode_summary;

    if items.is_empty() {
        return Vec::new();
    }

    let store = match handle.state.library.store.lock() {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };

    // For each queued episode ID, scan all_podcasts() for a matching episode.
    // O(ids * total_episodes) — acceptable: queues are short (typical 1-10 items)
    // and this path only runs when the playback domain rev advances (queue change
    // or position tick), NOT on every library mutation.
    //
    // Case-insensitive comparison: iOS sends UPPERCASE UUID strings; stored IDs
    // are lowercase (matching episode_playback_info behaviour in the store).
    let all_pods = store.all_podcasts();
    items
        .iter()
        .filter_map(|item| {
            let id_lower = item.episode_id.to_lowercase();
            for (podcast, episodes) in &all_pods {
                for ep in *episodes {
                    if ep.id.0.to_string() == id_lower {
                        let mut row = episode_summary(
                            handle,
                            &store,
                            podcast,
                            ep,
                            transcripts,
                            categories_cache,
                        );
                        row.queue_start_secs = item.start_secs;
                        row.queue_end_secs = item.end_secs;
                        row.queue_slot_id = Some(item.slot_id.clone());
                        return Some(row);
                    }
                }
            }
            None
        })
        .collect()
}

/// Build a [`WidgetSnapshot`] directly from the store, bypassing the full
/// library projection.
///
/// This is the slice-local equivalent of `build_widget_snapshot(now_playing, &library)`.
/// Instead of requiring the already-assembled `Vec<PodcastSummary>`, it scans
/// the store once to:
///  1. Sum `unplayed_count` across subscribed shows.
///  2. Resolve the now-playing episode's title/podcast_title/artwork/duration.
///
/// Output is byte-identical to `build_widget_snapshot` for the same store+player
/// state: the same fields, same fallback logic, same `clamp_fraction` calculation.
pub(super) fn build_widget_from_store(
    handle: &PodcastHandle,
    now_playing: Option<&crate::player::PlayerState>,
) -> Option<super::projections::WidgetSnapshot> {
    use super::projections::WidgetSnapshot;

    let store = match handle.state.library.store.lock() {
        Ok(s) => s,
        Err(_) => return None,
    };

    // Sum unplayed episodes across subscribed shows only.
    let mut unplayed_count: usize = 0;
    let mut episode_title: Option<String> = None;
    let mut podcast_title: Option<String> = None;
    let mut artwork_url: Option<String> = None;
    let mut is_playing = false;
    let mut position_secs = 0.0_f64;
    let mut duration_secs = 0.0_f64;
    let mut position_fraction = 0.0_f32;
    let mut chapter_title: Option<String> = None;

    let loaded = now_playing.filter(|s| s.episode_id.is_some());

    // Resolve episode metadata and unplayed count in a single store pass.
    for (podcast, episodes) in store.all_podcasts() {
        if store.is_subscribed(podcast.id) {
            unplayed_count += episodes.iter().filter(|e| !e.played).count();
        }
        if let Some(state) = loaded {
            if let Some(target_id) = state.episode_id.as_deref() {
                if episode_title.is_none() {
                    for ep in episodes {
                        if ep.id.0.to_string() == target_id {
                            episode_title = Some(ep.title.clone());
                            podcast_title = Some(podcast.title.clone());
                            artwork_url = ep.image_url.as_ref().map(|u: &url::Url| u.to_string())
                                .or_else(|| podcast.image_url.as_ref().map(|u: &url::Url| u.to_string()));
                            // Prefer feed duration over player duration (same as
                            // build_widget_snapshot / old iOS applyNowPlayingSnapshot).
                            if state.duration_secs <= 0.0 {
                                if let Some(feed_dur) = ep.duration_secs {
                                    if feed_dur > 0.0 {
                                        duration_secs = feed_dur;
                                    }
                                }
                            }
                            break;
                        }
                    }
                }
            }
        }
    }

    if let Some(state) = loaded {
        is_playing = state.is_playing;
        position_secs = state.position_secs;
        if duration_secs <= 0.0 {
            duration_secs = state.duration_secs;
        }
        chapter_title = state.current_chapter_title.clone();
        if episode_title.is_none() {
            episode_title = state.episode_id.as_deref().map(str::to_owned);
        }
        position_fraction =
            super::snapshot_widget::clamp_fraction_pub(position_secs, duration_secs);
    }

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
