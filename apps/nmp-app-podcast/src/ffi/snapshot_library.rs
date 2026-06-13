//! Library row projection for snapshot assembly.

use std::collections::HashMap;

use podcast_core::{Episode, Podcast};

use crate::ffi::handle::PodcastHandle;
use crate::ffi::projections::{ChapterSummary, EpisodeSummary, PodcastSummary, TranscriptEntry};
use crate::store::PodcastStore;

/// Build a single [`EpisodeSummary`] from one stored episode, populating every
/// derived field from the store + caches.
///
/// This is the SINGLE source of truth for episode-row construction. Both the
/// full-library projection (`build_library_snapshot`) and the slice-local
/// playback queue builder (`build_queue_rows_from_store`) call this so the rows
/// they emit are byte-identical by construction: same `clean_html`, same store
/// lookups (transcript / ad_segments / triage / metadata_indexed /
/// transcript_status), same LOWERCASE `ep.id.0.to_string()` id.
///
/// `transcripts` and `categories_cache` are the pre-snapshotted caches keyed by
/// the lowercase episode id string; callers pass the same maps they would pass
/// to `build_library_snapshot`.
pub(super) fn episode_summary(
    handle: &PodcastHandle,
    store: &PodcastStore,
    podcast: &Podcast,
    ep: &Episode,
    transcripts: &HashMap<String, Vec<TranscriptEntry>>,
    categories_cache: &HashMap<String, Vec<String>>,
) -> EpisodeSummary {
    let ep_id = ep.id.0.to_string();
    let transcript = store.transcript_for(&ep_id).map(str::to_owned);
    let transcript_entries = transcripts.get(&ep_id).cloned().unwrap_or_default();
    let ai_categories = categories_cache.get(&ep_id).cloned().unwrap_or_default();
    let ad_segments = store.ad_segments_for(&ep_id).to_vec();
    let triage = store.triage_for(&ep_id);
    let transcript_override = store.transcript_status_for(&ep_id);

    EpisodeSummary {
        id: ep_id.clone(),
        title: ep.title.clone(),
        podcast_id: Some(podcast.id.0.to_string()),
        podcast_title: Some(podcast.title.clone()),
        duration_secs: ep.duration_secs,
        artwork_url: ep.image_url.as_ref().map(|u| u.to_string()),
        published_at: Some(ep.pub_date.timestamp()),
        download_path: store.local_path_for(&ep.id).map(str::to_owned),
        file_size_bytes: store.file_size_for(&ep.id).unwrap_or(0),
        enclosure_url: Some(ep.enclosure_url.to_string()),
        description: Some(handle.clean_html(&ep.description)).filter(|d| !d.is_empty()),
        transcript,
        transcript_url: ep.publisher_transcript_url.as_ref().map(|u| u.to_string()),
        transcript_entries,
        chapters: ep
            .chapters
            .as_ref()
            .map(|cs| {
                cs.iter()
                    .map(|c| ChapterSummary {
                        start_secs: c.start_secs,
                        end_secs: c.end_secs,
                        title: c.title.clone(),
                        image_url: c.image_url.as_ref().map(|u| u.to_string()),
                        url: c.link_url.as_ref().map(|u| u.to_string()),
                        is_ai_generated: c.is_ai_generated,
                        source: c.source,
                        source_episode_id: c.source_episode_id.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default(),
        playback_position_secs: (ep.position_secs > 0.0).then_some(ep.position_secs),
        summary: ep.summary.clone(),
        ai_categories,
        ad_segments,
        played: ep.played,
        starred: ep.is_starred,
        triage_decision: triage.map(|(d, _, _)| d.clone()),
        triage_is_hero: triage.map(|(_, h, _)| *h).unwrap_or(false),
        triage_rationale: triage.and_then(|(_, _, r)| r.clone()),
        metadata_indexed: store.is_metadata_indexed(&ep_id),
        transcript_status: transcript_override
            .map(|(st, _)| st.clone())
            .unwrap_or_default(),
        transcript_status_message: transcript_override.and_then(|(_, m)| m.clone()),
    }
}

pub(super) fn build_library_snapshot(
    handle: &PodcastHandle,
    store: &PodcastStore,
    transcripts: &HashMap<String, Vec<TranscriptEntry>>,
    categories_cache: &HashMap<String, Vec<String>>,
) -> Vec<PodcastSummary> {
    store
        .all_podcasts()
        .into_iter()
        .map(|(podcast, episodes)| PodcastSummary {
            id: podcast.id.0.to_string(),
            title: podcast.title.clone(),
            episode_count: episodes.len(),
            unplayed_count: episodes.iter().filter(|e| !e.played).count(),
            is_subscribed: store.is_subscribed(podcast.id),
            artwork_url: podcast.image_url.as_ref().map(|u| u.to_string()),
            feed_url: podcast.feed_url.as_ref().map(|u| u.to_string()),
            author: if podcast.author.is_empty() {
                None
            } else {
                Some(podcast.author.clone())
            },
            description: Some(handle.clean_html(&podcast.description)).filter(|d| !d.is_empty()),
            last_refreshed_at: podcast.last_refreshed_at.map(|d| d.timestamp_millis()),
            title_is_placeholder: podcast.title_is_placeholder,
            owner_pubkey_hex: podcast.owner_pubkey_hex.clone(),
            nostr_visibility: match podcast.nostr_visibility {
                podcast_core::NostrVisibility::Private => "private".to_string(),
                podcast_core::NostrVisibility::Public => "public".to_string(),
            },
            auto_download: store.is_auto_download_enabled(podcast.id),
            cellular_allowed: !store.wifi_only_for(podcast.id),
            episodes: episodes
                .iter()
                .map(|ep| episode_summary(handle, store, podcast, ep, transcripts, categories_cache))
                .collect(),
        })
        .collect()
}
