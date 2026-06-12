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
    ///
    /// `feed_url` is threaded all the way down from `parse_feed` so the
    /// resulting [`Episode`]'s id is the same UUIDv5 every time we re-parse
    /// the same item from the same feed (see
    /// [`podcast_core::types::episode::EpisodeId::from_feed_and_guid`]).
    pub fn make_episode(self, podcast_id: PodcastId, feed_url: &str) -> Option<Episode> {
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
            feed_url,
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
///
/// Rejects non-finite (`NaN`, `Inf`, `-Inf`) and negative values so a
/// malformed feed cannot propagate a NaN into chapter math, which would
/// serialise required float fields as JSON `null` and drop the entire
/// `PodcastUpdate` frame on the Swift side.
pub fn parse_duration(raw: &str) -> Option<f64> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let parts: Vec<&str> = trimmed.split(':').collect();
    let seconds = if parts.len() == 1 {
        parts[0].parse::<f64>().ok()?
    } else {
        let mut acc: f64 = 0.0;
        for part in parts {
            let value: f64 = part.parse().ok()?;
            acc = acc * 60.0 + value;
        }
        acc
    };
    // Reject NaN, Inf, -Inf, and negative durations — none are valid
    // episode lengths and all would corrupt downstream float math.
    if seconds.is_finite() && seconds >= 0.0 {
        Some(seconds)
    } else {
        None
    }
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
#[path = "accumulator_tests.rs"]
mod tests;
