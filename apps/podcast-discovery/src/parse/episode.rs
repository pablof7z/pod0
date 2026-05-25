//! Parse a `kind:30075` Nostr event into a [`NIP74Episode`].
//!
//! Tag layout is captured in `App/Sources/Services/
//! NostrPodcastDiscoveryService.parseEpisode(from:podcastID:)` and
//! `NostrPodcastPublisher.publishEpisode` — this module ports the wire
//! → typed direction. The reverse direction lives in `build/episode.rs`,
//! and the parsed view → `podcast_core::Episode` mapping lives in
//! `parse/episode_map.rs`.

use crate::kinds::KIND_EPISODE;
use crate::parse::imeta::parse_imeta_fields;
use crate::parse::{first_tag, first_tag_value};
use crate::types::{NIP74Episode, ParseError, ShowReference};

/// Parse a Nostr event's header + tags into a raw [`NIP74Episode`].
///
/// `kind` is checked against [`KIND_EPISODE`]. `created_at` is the event
/// header timestamp (unix seconds), used as the fallback for
/// `published_at` when the tag is missing or non-numeric. `content` is
/// the event content string — used as a fallback for the summary when no
/// `["summary", ...]` tag is present (matches the Swift discovery
/// service).
pub fn parse_episode_event(
    kind: u32,
    created_at: i64,
    content: &str,
    tags: &[Vec<String>],
) -> Result<NIP74Episode, ParseError> {
    if kind != KIND_EPISODE {
        return Err(ParseError::WrongKind {
            expected: KIND_EPISODE,
            got: kind,
        });
    }
    let d_tag = first_tag_value(tags, "d")
        .ok_or(ParseError::MissingTag("d"))?
        .to_string();

    let title = first_tag_value(tags, "title")
        .map(str::to_string)
        .unwrap_or_default();
    let summary = first_tag_value(tags, "summary")
        .map(str::to_string)
        .unwrap_or_else(|| content.to_string());
    let image_url = first_tag_value(tags, "image").map(str::to_string);

    // `published_at` falls back to event header `created_at` when missing
    // or non-numeric (mirrors Swift parseEpisode).
    let published_at = first_tag_value(tags, "published_at")
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(created_at);

    let duration_secs = first_tag_value(tags, "duration").and_then(|v| v.parse::<f64>().ok());

    // Parse the imeta block (preferred audio source).
    let imeta_fields = first_tag(tags, "imeta")
        .map(parse_imeta_fields)
        .unwrap_or_default();
    let audio_url = imeta_fields
        .url
        .or_else(|| first_tag_value(tags, "url").map(str::to_string))
        .ok_or(ParseError::MissingAudioUrl)?;

    let show_a_tag = first_tag_value(tags, "a")
        .map(parse_a_reference)
        .transpose()?;

    let chapters_url = first_tag_value(tags, "chapters").map(str::to_string);
    let transcript_tag = first_tag(tags, "transcript");
    let transcript_url = transcript_tag
        .and_then(|t| t.get(1))
        .filter(|s| !s.is_empty())
        .cloned();
    let transcript_mime_type = transcript_tag
        .and_then(|t| t.get(2))
        .filter(|s| !s.is_empty())
        .cloned();

    Ok(NIP74Episode {
        d_tag,
        title,
        summary,
        published_at,
        duration_secs,
        image_url,
        audio_url,
        audio_mime_type: imeta_fields.mime,
        audio_sha256_hex: imeta_fields.sha256,
        audio_size_bytes: imeta_fields.size,
        show_a_tag,
        chapters_url,
        transcript_url,
        transcript_mime_type,
    })
}

fn parse_a_reference(value: &str) -> Result<ShowReference, ParseError> {
    let mut parts = value.splitn(3, ':');
    let kind_str = parts
        .next()
        .ok_or_else(|| ParseError::MalformedReference(value.into()))?;
    let pubkey = parts
        .next()
        .ok_or_else(|| ParseError::MalformedReference(value.into()))?;
    let d_tag = parts
        .next()
        .ok_or_else(|| ParseError::MalformedReference(value.into()))?;
    let kind = kind_str
        .parse::<u32>()
        .map_err(|_| ParseError::MalformedReference(value.into()))?;
    if pubkey.is_empty() || d_tag.is_empty() {
        return Err(ParseError::MalformedReference(value.into()));
    }
    Ok(ShowReference {
        kind,
        pubkey: pubkey.to_string(),
        d_tag: d_tag.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal_tags() -> Vec<Vec<String>> {
        vec![
            vec!["d".into(), "ep-1".into()],
            vec!["title".into(), "Pilot".into()],
            vec!["imeta".into(), "url https://media.example/ep-1.m4a".into()],
        ]
    }

    #[test]
    fn parse_minimal_episode_succeeds() {
        let ep = parse_episode_event(KIND_EPISODE, 999, "", &minimal_tags()).expect("parse");
        assert_eq!(ep.d_tag, "ep-1");
        assert_eq!(ep.title, "Pilot");
        assert_eq!(ep.audio_url, "https://media.example/ep-1.m4a");
        assert_eq!(ep.published_at, 999); // falls back to created_at
        assert!(ep.duration_secs.is_none());
        assert!(ep.audio_sha256_hex.is_none());
        assert!(ep.show_a_tag.is_none());
    }

    #[test]
    fn parse_full_episode_collects_imeta_and_a_tag() {
        let tags = vec![
            vec!["d".into(), "ep-1".into()],
            vec!["title".into(), "Pilot".into()],
            vec!["summary".into(), "First episode".into()],
            vec!["published_at".into(), "1700000123".into()],
            vec!["a".into(), "30074:agent-pk:show-1".into()],
            vec!["duration".into(), "1800".into()],
            vec!["image".into(), "https://img.example/ep-1.jpg".into()],
            vec![
                "imeta".into(),
                "url https://media.example/ep-1.m4a".into(),
                "m audio/mp4".into(),
                "x deadbeef".into(),
                "size 12345".into(),
            ],
            vec![
                "chapters".into(),
                "https://chapters.example/ep-1.json".into(),
                "application/json+chapters".into(),
            ],
            vec![
                "transcript".into(),
                "https://tx.example/ep-1.vtt".into(),
                "text/vtt".into(),
            ],
        ];
        let ep = parse_episode_event(KIND_EPISODE, 0, "ignored content", &tags).expect("parse");
        assert_eq!(ep.published_at, 1_700_000_123);
        assert_eq!(ep.duration_secs, Some(1800.0));
        assert_eq!(ep.audio_mime_type.as_deref(), Some("audio/mp4"));
        assert_eq!(ep.audio_sha256_hex.as_deref(), Some("deadbeef"));
        assert_eq!(ep.audio_size_bytes, Some(12_345));
        assert_eq!(ep.image_url.as_deref(), Some("https://img.example/ep-1.jpg"));
        let show_ref = ep.show_a_tag.expect("a tag present");
        assert_eq!(show_ref.kind, 30074);
        assert_eq!(show_ref.pubkey, "agent-pk");
        assert_eq!(show_ref.d_tag, "show-1");
        assert_eq!(ep.chapters_url.as_deref(), Some("https://chapters.example/ep-1.json"));
        assert_eq!(ep.transcript_url.as_deref(), Some("https://tx.example/ep-1.vtt"));
        assert_eq!(ep.transcript_mime_type.as_deref(), Some("text/vtt"));
        assert_eq!(ep.summary, "First episode");
    }

    #[test]
    fn parse_rejects_wrong_kind() {
        let err = parse_episode_event(KIND_EPISODE + 1, 0, "", &minimal_tags()).unwrap_err();
        assert!(matches!(err, ParseError::WrongKind { .. }));
    }

    #[test]
    fn parse_requires_audio_url() {
        let tags = vec![
            vec!["d".into(), "ep-1".into()],
            vec!["title".into(), "Pilot".into()],
        ];
        let err = parse_episode_event(KIND_EPISODE, 0, "", &tags).unwrap_err();
        assert_eq!(err, ParseError::MissingAudioUrl);
    }

    #[test]
    fn parse_falls_back_to_url_tag_when_imeta_missing() {
        let tags = vec![
            vec!["d".into(), "ep-1".into()],
            vec!["title".into(), "Pilot".into()],
            vec!["url".into(), "https://media.example/legacy.mp3".into()],
        ];
        let ep = parse_episode_event(KIND_EPISODE, 0, "", &tags).expect("parse");
        assert_eq!(ep.audio_url, "https://media.example/legacy.mp3");
    }

    #[test]
    fn parse_rejects_malformed_a_tag() {
        let tags = vec![
            vec!["d".into(), "ep-1".into()],
            vec!["title".into(), "Pilot".into()],
            vec!["a".into(), "no-colons".into()],
            vec!["imeta".into(), "url https://x".into()],
        ];
        let err = parse_episode_event(KIND_EPISODE, 0, "", &tags).unwrap_err();
        assert!(matches!(err, ParseError::MalformedReference(_)));
    }
}
