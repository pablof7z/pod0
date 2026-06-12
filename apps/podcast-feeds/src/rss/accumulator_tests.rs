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

/// Guard: a feed with `<itunes:duration>NaN</itunes:duration>` must yield
/// `None` instead of `Some(NaN)`.  Without this, NaN propagates into chapter
/// math (`ai_chapters::stub_chapters` divides by `duration_secs`), which
/// serialises `ChapterSummary::start_secs` as JSON `null`, causing the Swift
/// bridge to throw `keyNotFound` and drop the entire `PodcastUpdate` frame.
#[test]
fn parse_duration_rejects_nan() {
    assert_eq!(parse_duration("NaN"), None, "NaN must be rejected at the inlet");
    assert_eq!(parse_duration("nan"), None);
}

#[test]
fn parse_duration_rejects_infinity() {
    assert_eq!(parse_duration("inf"), None, "inf must be rejected at the inlet");
    assert_eq!(parse_duration("Inf"), None);
    assert_eq!(parse_duration("-inf"), None);
    assert_eq!(parse_duration("infinity"), None);
}

#[test]
fn parse_duration_rejects_negative() {
    assert_eq!(parse_duration("-1"), None, "negative durations are invalid");
    assert_eq!(parse_duration("-3600"), None);
}

#[test]
fn parse_duration_accepts_zero() {
    // Zero is technically valid (e.g. a pre-release episode stub with no content).
    assert_eq!(parse_duration("0"), Some(0.0));
    assert_eq!(parse_duration("0:00"), Some(0.0));
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
