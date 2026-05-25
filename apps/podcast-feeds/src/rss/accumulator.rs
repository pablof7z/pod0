use chrono::{DateTime, TimeZone, Utc};
use podcast_core::types::transcript::TranscriptKind;
use podcast_core::{Episode, Person, PodcastId, SoundBite};
use url::Url;

use crate::rss::date::parse_rfc2822;

/// Mutable scratch state for a single `<item>` while the RSS parser walks its
/// children. Ported from `RSSItemAccumulator.swift`.
#[derive(Debug, Default, Clone)]
pub struct RssItemAccumulator {
    pub title: String,
    pub description: String,
    pub pub_date_raw: Option<String>,
    pub guid: Option<String>,
    pub duration_secs: Option<f64>,
    pub enclosure_url: Option<Url>,
    pub enclosure_mime_type: Option<String>,
    pub itunes_image_url: Option<Url>,

    pub preferred_transcript: Option<PreferredTranscript>,
    pub chapters_url: Option<Url>,

    pub persons: Vec<Person>,
    pub sound_bites: Vec<SoundBite>,

    pub pending_person: Option<Person>,
    pub pending_soundbite_start: Option<f64>,
    pub pending_soundbite_duration: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct PreferredTranscript {
    pub url: Url,
    pub kind: Option<TranscriptKind>,
}

impl RssItemAccumulator {
    /// Stable floor date for missing/malformed `pubDate`. Mirrors the Swift
    /// `fallbackPubDate = Date(timeIntervalSince1970: 0)` so broken feeds
    /// don't surface as "new now" on every refresh.
    pub fn fallback_pub_date() -> DateTime<Utc> {
        Utc.timestamp_opt(0, 0).unwrap()
    }

    /// Returns `None` when the item lacks an `<enclosure>` URL — common in
    /// hybrid blog/podcast feeds and not playable.
    pub fn make_episode(self, podcast_id: PodcastId) -> Option<Episode> {
        let enclosure_url = self.enclosure_url.clone()?;

        let resolved_guid = match self.guid.as_ref() {
            Some(g) if !g.is_empty() => g.clone(),
            _ => synthesized_guid(Some(&enclosure_url), self.pub_date_raw.as_deref()),
        };

        let pub_date = self
            .pub_date_raw
            .as_deref()
            .and_then(parse_rfc2822)
            .unwrap_or_else(Self::fallback_pub_date);

        let mut episode = Episode::new(
            podcast_id,
            resolved_guid,
            self.title.trim().to_string(),
            enclosure_url,
            pub_date,
        );
        episode.description = self.description;
        episode.duration_secs = self.duration_secs;
        episode.enclosure_mime_type = self.enclosure_mime_type;
        episode.image_url = self.itunes_image_url;
        episode.persons = (!self.persons.is_empty()).then_some(self.persons);
        episode.sound_bites = (!self.sound_bites.is_empty()).then_some(self.sound_bites);
        if let Some(transcript) = self.preferred_transcript {
            episode.publisher_transcript_url = Some(transcript.url);
            episode.publisher_transcript_type = transcript.kind;
        }
        episode.chapters_url = self.chapters_url;
        Some(episode)
    }
}

/// Synthesizes a deterministic GUID for items missing `<guid>`. Combines the
/// enclosure URL and the raw pubDate string so a re-fetch produces the same
/// id. Lane 6 keys embeddings off `Episode.guid`, so stability is
/// load-bearing — keep the exact format used in the Swift original:
/// `"synth::{enclosure}::{rawDate}"`.
pub fn synthesized_guid(enclosure: Option<&Url>, pub_date_raw: Option<&str>) -> String {
    let enclosure_part = enclosure
        .map(|u| u.as_str().to_string())
        .unwrap_or_else(|| "no-enclosure".to_string());
    let date_part = pub_date_raw
        .map(|d| d.trim().to_string())
        .unwrap_or_else(|| "no-date".to_string());
    format!("synth::{enclosure_part}::{date_part}")
}

/// Higher rank wins. JSON > VTT > SRT > HTML > text > unknown. Matches the
/// Swift `transcriptRank` ordering exactly.
pub fn transcript_rank(kind: Option<TranscriptKind>) -> u8 {
    match kind {
        Some(TranscriptKind::Json) => 5,
        Some(TranscriptKind::Vtt) => 4,
        Some(TranscriptKind::Srt) => 3,
        Some(TranscriptKind::Html) => 2,
        Some(TranscriptKind::Text) => 1,
        None => 0,
    }
}

/// Parses iTunes durations: `H:MM:SS`, `MM:SS`, or raw seconds.
pub fn parse_duration(raw: &str) -> Option<f64> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let parts: Vec<&str> = trimmed.split(':').collect();
    if parts.len() == 1 {
        return parts[0].parse::<f64>().ok();
    }
    let mut seconds: f64 = 0.0;
    for part in parts {
        let value: f64 = part.parse().ok()?;
        seconds = seconds * 60.0 + value;
    }
    Some(seconds)
}

/// Resolves a URL against the feed URL. Handles three cases:
/// 1. Protocol-relative (`//host/path`) — borrow the feed's scheme.
/// 2. Absolute (`https://...`) — use as-is.
/// 3. Relative (`/path` or `path`) — join against the feed URL.
pub fn resolve_url(raw: &str, feed_url: &Url) -> Option<Url> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(stripped) = trimmed.strip_prefix("//") {
        let scheme = feed_url.scheme();
        return Url::parse(&format!("{scheme}://{stripped}")).ok();
    }
    if let Ok(absolute) = Url::parse(trimmed) {
        return Some(absolute);
    }
    feed_url.join(trimmed).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synthesized_guid_format_is_stable() {
        let url = Url::parse("https://example.com/audio.mp3").unwrap();
        let guid = synthesized_guid(Some(&url), Some("Tue, 01 Jan 2024 12:00:00 +0000"));
        assert_eq!(
            guid,
            "synth::https://example.com/audio.mp3::Tue, 01 Jan 2024 12:00:00 +0000"
        );
    }

    #[test]
    fn synthesized_guid_with_no_enclosure_or_date() {
        let guid = synthesized_guid(None, None);
        assert_eq!(guid, "synth::no-enclosure::no-date");
    }

    #[test]
    fn transcript_rank_orders_correctly() {
        assert!(transcript_rank(Some(TranscriptKind::Json)) > transcript_rank(Some(TranscriptKind::Vtt)));
        assert!(transcript_rank(Some(TranscriptKind::Vtt)) > transcript_rank(Some(TranscriptKind::Srt)));
        assert!(transcript_rank(Some(TranscriptKind::Srt)) > transcript_rank(Some(TranscriptKind::Html)));
        assert!(transcript_rank(Some(TranscriptKind::Html)) > transcript_rank(Some(TranscriptKind::Text)));
        assert!(transcript_rank(Some(TranscriptKind::Text)) > transcript_rank(None));
    }

    #[test]
    fn parse_duration_handles_three_formats() {
        assert_eq!(parse_duration("3600"), Some(3600.0));
        assert_eq!(parse_duration("12:34"), Some(12.0 * 60.0 + 34.0));
        assert_eq!(parse_duration("1:02:03"), Some(3723.0));
        assert_eq!(parse_duration(""), None);
        assert_eq!(parse_duration("bogus"), None);
    }

    #[test]
    fn resolve_url_protocol_relative() {
        let feed = Url::parse("https://example.com/feed.xml").unwrap();
        let resolved = resolve_url("//cdn.example.com/img.jpg", &feed).unwrap();
        assert_eq!(resolved.as_str(), "https://cdn.example.com/img.jpg");
    }

    #[test]
    fn resolve_url_absolute_passthrough() {
        let feed = Url::parse("https://example.com/feed.xml").unwrap();
        let resolved = resolve_url("https://other.example/file", &feed).unwrap();
        assert_eq!(resolved.as_str(), "https://other.example/file");
    }

    #[test]
    fn resolve_url_relative_joins_against_feed() {
        let feed = Url::parse("https://example.com/feeds/main.xml").unwrap();
        let resolved = resolve_url("/img.jpg", &feed).unwrap();
        assert_eq!(resolved.as_str(), "https://example.com/img.jpg");
    }

    #[test]
    fn fallback_pub_date_is_epoch() {
        let d = RssItemAccumulator::fallback_pub_date();
        assert_eq!(d.timestamp(), 0);
    }
}
