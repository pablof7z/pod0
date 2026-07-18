use std::borrow::Cow;

use pod0_domain::{EpisodeId, PodcastId, PublisherTranscriptFormat};
use quick_xml::XmlVersion;
use quick_xml::events::{BytesEnd, BytesStart};
use sha2::{Digest, Sha256};
use time::{OffsetDateTime, format_description::well_known::Rfc2822};

pub(super) fn name(element: &BytesStart<'_>) -> String {
    String::from_utf8_lossy(element.name().as_ref()).to_ascii_lowercase()
}

pub(super) fn name_end(element: &BytesEnd<'_>) -> String {
    String::from_utf8_lossy(element.name().as_ref()).to_ascii_lowercase()
}

pub(super) fn attribute(element: &BytesStart<'_>, key: &[u8]) -> Option<String> {
    element
        .attributes()
        .with_checks(false)
        .filter_map(Result::ok)
        .find(|attribute| attribute.key.as_ref().eq_ignore_ascii_case(key))
        .and_then(|attribute| {
            attribute
                .normalized_value(XmlVersion::Implicit1_0)
                .ok()
                .map(Cow::into_owned)
        })
}

pub(super) fn nonempty(value: &str) -> Option<String> {
    (!value.is_empty()).then(|| value.to_owned())
}

pub(super) fn milliseconds(value: Option<&str>) -> Option<u64> {
    let value: f64 = value?.parse().ok()?;
    (value.is_finite() && value >= 0.0).then(|| (value * 1_000.0).round() as u64)
}

pub(super) fn duration(value: &str) -> Option<u64> {
    let mut seconds = 0.0;
    for part in value.split(':') {
        seconds = seconds * 60.0 + part.parse::<f64>().ok()?;
    }
    (seconds.is_finite() && seconds >= 0.0).then(|| (seconds * 1_000.0).round() as u64)
}

pub(super) fn parse_date(value: Option<&str>) -> i64 {
    value
        .and_then(|value| OffsetDateTime::parse(value, &Rfc2822).ok())
        .and_then(|date| i64::try_from(date.unix_timestamp_nanos() / 1_000_000).ok())
        .unwrap_or(0)
}

pub(super) fn episode_id(podcast_id: PodcastId, guid: &str) -> EpisodeId {
    let mut hash = Sha256::new();
    hash.update(podcast_id.into_bytes());
    hash.update(guid.as_bytes());
    EpisodeId::from_bytes(hash.finalize()[..16].try_into().expect("digest slice"))
}

pub(super) fn transcript_format(value: Option<&str>) -> PublisherTranscriptFormat {
    let value = value.unwrap_or("").trim().to_ascii_lowercase();
    if value.starts_with("application/json") {
        PublisherTranscriptFormat::Json
    } else if value.starts_with("text/vtt") || value == "application/vtt" || value == "vtt" {
        PublisherTranscriptFormat::WebVtt
    } else if matches!(
        value.as_str(),
        "application/x-subrip" | "application/srt" | "text/srt" | "srt"
    ) {
        PublisherTranscriptFormat::SubRip
    } else if value.starts_with("text/html") || value == "html" {
        PublisherTranscriptFormat::Html
    } else if value.starts_with("text/plain") || value == "plain" {
        PublisherTranscriptFormat::PlainText
    } else {
        PublisherTranscriptFormat::Unknown
    }
}

pub(super) fn transcript_rank(value: &PublisherTranscriptFormat) -> u8 {
    match value {
        PublisherTranscriptFormat::Json => 5,
        PublisherTranscriptFormat::WebVtt => 4,
        PublisherTranscriptFormat::SubRip => 3,
        PublisherTranscriptFormat::Html => 2,
        PublisherTranscriptFormat::PlainText => 1,
        PublisherTranscriptFormat::Unknown | PublisherTranscriptFormat::Unsupported { .. } => 0,
    }
}
