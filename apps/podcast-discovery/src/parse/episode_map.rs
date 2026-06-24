//! Map a parsed [`NipF4DiscoveryEpisode`] onto an [`Episode`] domain row.
//!
//! Kept in its own file so the wire-shape parser (`parse/episode.rs`)
//! and the domain-shape mapping live next to each other but stay under
//! AGENTS.md's 300-LOC soft limit. The mapping is total (no `Result`)
//! because every parse failure is already surfaced at the
//! [`parse_episode_event`] boundary — once you have an `NipF4DiscoveryEpisode`,
//! producing an `Episode` cannot fail.
//!
//! [`parse_episode_event`]: super::episode::parse_episode_event

use chrono::{TimeZone, Utc};
use podcast_core::types::episode::{Episode, EpisodeId};
use podcast_core::types::podcast::PodcastId;
use url::Url;
use uuid::Uuid;

use crate::types::NipF4DiscoveryEpisode;

/// Map a parsed [`NipF4DiscoveryEpisode`] onto an [`Episode`] domain row.
///
/// `podcast_id` is supplied by the caller — typically derived from
/// [`crate::parse::show::show_to_podcast`] on the parent show so the
/// foreign-key relation survives.
pub fn episode_to_episode(ep: &NipF4DiscoveryEpisode, podcast_id: PodcastId) -> Episode {
    let pub_date = Utc
        .timestamp_opt(ep.published_at, 0)
        .single()
        .unwrap_or_else(Utc::now);
    let audio_url = Url::parse(&ep.audio_url).unwrap_or_else(|_| {
        // Fallback: opaque URL wrapper so the domain row is still valid.
        // The publisher always produces a parseable URL, so this branch
        // is defensive — matches the Swift `URL(string:)?` discard-on-fail
        // shape (which would drop the event upstream).
        Url::parse("about:invalid").expect("about:invalid is a valid URL")
    });
    let title = if ep.title.is_empty() {
        "Untitled Episode".to_string()
    } else {
        ep.title.clone()
    };

    // `Episode::new` derives a UUIDv5 from `(feed_url, guid)`; we immediately
    // override `episode.id` with the NIP-F4 event id below, so the
    // placeholder `"nip74"` namespace string is a stable but unused input.
    let mut episode = Episode::new(
        podcast_id,
        "nip74",
        ep.d_tag.clone(),
        title,
        audio_url,
        pub_date,
    );
    episode.id = EpisodeId::new(episode_id_from_d_tag(&ep.d_tag));
    episode.description = ep.summary.clone();
    episode.duration_secs = ep.duration_secs;
    episode.enclosure_mime_type = ep.audio_mime_type.clone();
    episode.image_url = ep.image_url.as_deref().and_then(|s| Url::parse(s).ok());
    episode.chapters_url = ep.chapters_url.as_deref().and_then(|s| Url::parse(s).ok());
    episode.publisher_transcript_url =
        ep.transcript_url.as_deref().and_then(|s| Url::parse(s).ok());
    episode.publisher_transcript_type = ep
        .transcript_mime_type
        .as_deref()
        .and_then(podcast_core::types::transcript::TranscriptKind::from_mime);
    episode
}

/// Stable UUIDv5 over the episode `d` tag value so the `Episode.id` is
/// reproducible for the same wire event.
fn episode_id_from_d_tag(d_tag: &str) -> Uuid {
    // Namespace UUID scoped to NIP-F4 episode d-tags.
    const NS: Uuid = Uuid::from_bytes([
        0xc6, 0x10, 0xa0, 0xf9, 0xe4, 0x21, 0x5e, 0xfb, 0x90, 0x6c, 0x5d, 0x88, 0x6a, 0x7e, 0x4b,
        0x10,
    ]);
    Uuid::new_v5(&NS, d_tag.as_bytes())
}

#[cfg(test)]
#[path = "episode_map_tests.rs"]
mod tests;
