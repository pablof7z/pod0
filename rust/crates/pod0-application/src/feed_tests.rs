use pod0_domain::{PodcastId, PublisherTranscriptFormat, UnixTimestampMilliseconds};

use crate::{FeedParseFailure, normalize_feed_url, parse_podcast_feed};

#[test]
fn rss_and_podcasting_two_metadata_match_the_native_fixture_behavior() {
    let feed = parse_podcast_feed(
        FIXTURE.as_bytes(),
        normalize_feed_url("https://example.com/podcasts/feed.xml").unwrap(),
        PodcastId::from_parts(1, 2),
        UnixTimestampMilliseconds::new(1_800_000_000_000),
    )
    .unwrap();

    assert_eq!(feed.podcast.title, "Example Show");
    assert_eq!(feed.podcast.author, "Test Author");
    assert_eq!(feed.podcast.language.as_deref(), Some("en-US"));
    assert_eq!(feed.podcast.categories, ["Technology", "News"]);
    assert_eq!(
        feed.podcast.image_url.as_deref(),
        Some("https://example.com/cover.jpg")
    );
    assert_eq!(feed.episodes.len(), 2);
    let first = &feed.episodes[0];
    assert_eq!(first.publisher_guid, "ep-0001");
    assert_eq!(first.duration_milliseconds, Some(5_025_000));
    assert_eq!(first.published_at.value, 1_777_885_200_000);
    assert_eq!(first.feed_metadata.persons.len(), 2);
    assert_eq!(first.feed_metadata.persons[0].name, "Alice Host");
    assert_eq!(
        first.feed_metadata.sound_bites[0].start_milliseconds,
        120_500
    );
    assert_eq!(
        first.feed_metadata.chapters_url.as_deref(),
        Some("https://example.com/chapters/ep1.json")
    );
    let transcript = first.feed_metadata.publisher_transcript.as_ref().unwrap();
    assert_eq!(transcript.url, "https://example.com/transcripts/ep1.json");
    assert_eq!(transcript.format, PublisherTranscriptFormat::Json);
    assert_eq!(
        feed.episodes[1].publisher_guid,
        "synth::https://example.com/audio/ep2.mp3::Tue, 05 May 2026 09:00:00 GMT"
    );
}

#[test]
fn parser_rejects_malformed_or_non_channel_xml_and_ignores_unplayable_items() {
    let identity = normalize_feed_url("https://example.test/feed").unwrap();
    let id = PodcastId::from_parts(0, 1);
    assert_eq!(
        parse_podcast_feed(
            b"<rss>",
            identity.clone(),
            id,
            UnixTimestampMilliseconds::new(0)
        ),
        Err(FeedParseFailure::MalformedXml)
    );
    assert_eq!(
        parse_podcast_feed(
            b"<rss></rss>",
            identity.clone(),
            id,
            UnixTimestampMilliseconds::new(0)
        ),
        Err(FeedParseFailure::MissingChannel)
    );
    let empty = parse_podcast_feed(
        b"<rss><channel><title>Show</title><item><guid>x</guid></item></channel></rss>",
        identity,
        id,
        UnixTimestampMilliseconds::new(0),
    )
    .unwrap();
    assert!(empty.episodes.is_empty());
}

const FIXTURE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0" xmlns:itunes="http://www.itunes.com/dtds/podcast-1.0.dtd"
 xmlns:podcast="https://podcastindex.org/namespace/1.0"
 xmlns:content="http://purl.org/rss/1.0/modules/content/">
<channel>
 <title>Example Show</title><description>A test feed.</description><language>en-US</language>
 <itunes:author>Test Author</itunes:author><itunes:image href="https://example.com/cover.jpg"/>
 <itunes:category text="Technology"/><itunes:category text="News"/>
 <item>
  <title>Episode 1</title><description><![CDATA[<p>Notes</p>]]></description>
  <pubDate>Mon, 04 May 2026 09:00:00 GMT</pubDate><guid>ep-0001</guid>
  <itunes:duration>1:23:45</itunes:duration>
  <enclosure url="https://example.com/audio/ep1.mp3" type="audio/mpeg"/>
  <podcast:transcript url="../transcripts/ep1.vtt" type="text/vtt"/>
  <podcast:transcript url="../transcripts/ep1.json" type="application/json"/>
  <podcast:chapters url="../chapters/ep1.json"/>
  <podcast:person role="host" href="/host">Alice Host</podcast:person>
  <podcast:person role="guest">Bob Guest</podcast:person>
  <podcast:soundbite startTime="120.5" duration="30">Hot take</podcast:soundbite>
 </item>
 <item><title>Episode 2</title><pubDate>Tue, 05 May 2026 09:00:00 GMT</pubDate>
  <enclosure url="https://example.com/audio/ep2.mp3" type="audio/mpeg"/></item>
</channel></rss>"#;
