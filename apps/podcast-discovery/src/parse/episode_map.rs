//! Map a parsed [`NIP74Episode`] onto an [`Episode`] domain row.
//!
//! Kept in its own file so the wire-shape parser (`parse/episode.rs`)
//! and the domain-shape mapping live next to each other but stay under
//! AGENTS.md's 300-LOC soft limit. The mapping is total (no `Result`)
//! because every parse failure is already surfaced at the
//! [`parse_episode_event`] boundary — once you have an `NIP74Episode`,
//! producing an `Episode` cannot fail.
//!
//! [`parse_episode_event`]: super::episode::parse_episode_event

use chrono::{TimeZone, Utc};
use podcast_core::types::episode::{Episode, EpisodeId};
use podcast_core::types::podcast::PodcastId;
use url::Url;
use uuid::Uuid;

use crate::types::NIP74Episode;

/// Map a parsed [`NIP74Episode`] onto an [`Episode`] domain row.
///
/// `podcast_id` is supplied by the caller — typically derived from
/// [`crate::parse::show::show_to_podcast`] on the parent show so the
/// foreign-key relation survives.
pub fn episode_to_episode(ep: &NIP74Episode, podcast_id: PodcastId) -> Episode {
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

    let mut episode = Episode::new(podcast_id, ep.d_tag.clone(), title, audio_url, pub_date);
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
    // Namespace UUID scoped to NIP-74 episode d-tags.
    const NS: Uuid = Uuid::from_bytes([
        0xc6, 0x10, 0xa0, 0xf9, 0xe4, 0x21, 0x5e, 0xfb, 0x90, 0x6c, 0x5d, 0x88, 0x6a, 0x7e, 0x4b,
        0x10,
    ]);
    Uuid::new_v5(&NS, d_tag.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use podcast_core::types::transcript::TranscriptKind;

    fn make_nip74() -> NIP74Episode {
        NIP74Episode {
            d_tag: "ep-1".into(),
            title: "Pilot".into(),
            summary: "First".into(),
            published_at: 1_700_000_000,
            duration_secs: Some(120.5),
            image_url: Some("https://img.example/e.jpg".into()),
            audio_url: "https://m.example/ep.m4a".into(),
            audio_mime_type: Some("audio/mp4".into()),
            audio_sha256_hex: None,
            audio_size_bytes: None,
            show_a_tag: None,
            chapters_url: Some("https://c.example/c.json".into()),
            transcript_url: Some("https://t.example/t.vtt".into()),
            transcript_mime_type: Some("text/vtt".into()),
        }
    }

    #[test]
    fn maps_every_supported_field() {
        let nip = make_nip74();
        let pid = PodcastId::generate();
        let ep = episode_to_episode(&nip, pid);
        assert_eq!(ep.podcast_id, pid);
        assert_eq!(ep.guid, "ep-1");
        assert_eq!(ep.title, "Pilot");
        assert_eq!(ep.description, "First");
        assert_eq!(ep.duration_secs, Some(120.5));
        assert_eq!(ep.enclosure_url.as_str(), "https://m.example/ep.m4a");
        assert_eq!(ep.enclosure_mime_type.as_deref(), Some("audio/mp4"));
        assert!(ep.image_url.is_some());
        assert!(ep.chapters_url.is_some());
        assert!(ep.publisher_transcript_url.is_some());
        assert!(matches!(ep.publisher_transcript_type, Some(TranscriptKind::Vtt)));
    }

    #[test]
    fn id_is_stable_per_d_tag() {
        let nip = make_nip74();
        let pid = PodcastId::generate();
        let a = episode_to_episode(&nip, pid);
        let b = episode_to_episode(&nip, pid);
        assert_eq!(a.id, b.id);
    }

    #[test]
    fn empty_title_yields_untitled_episode() {
        let mut nip = make_nip74();
        nip.title = String::new();
        let ep = episode_to_episode(&nip, PodcastId::generate());
        assert_eq!(ep.title, "Untitled Episode");
    }
}
