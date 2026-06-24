//! Parse a `kind:54` Nostr event (NIP-F4) into a [`NipF4DiscoveryEpisode`].
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
use crate::types::{NipF4DiscoveryEpisode, ParseError, ShowReference};

/// Parse a Nostr event's header + tags into a raw [`NipF4DiscoveryEpisode`].
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
) -> Result<NipF4DiscoveryEpisode, ParseError> {
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

    Ok(NipF4DiscoveryEpisode {
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
#[path = "episode_tests.rs"]
mod tests;
