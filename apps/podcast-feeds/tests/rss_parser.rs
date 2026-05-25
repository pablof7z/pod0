use podcast_core::types::transcript::TranscriptKind;
use podcast_core::PodcastId;
use podcast_feeds::rss::parse_feed;
use url::Url;

const MINIMAL_FEED: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0"
     xmlns:itunes="http://www.itunes.com/dtds/podcast-1.0.dtd"
     xmlns:podcast="https://podcastindex.org/namespace/1.0"
     xmlns:content="http://purl.org/rss/1.0/modules/content/">
  <channel>
    <title>Test Cast</title>
    <link>https://example.com/show</link>
    <language>en-us</language>
    <description>A small show for testing.</description>
    <itunes:author>Test Host</itunes:author>
    <itunes:image href="https://example.com/cover.jpg" />
    <itunes:category text="Technology" />
    <itunes:category text="News" />
    <item>
      <title>Episode 1</title>
      <description>First.</description>
      <pubDate>Mon, 01 Jan 2024 12:00:00 +0000</pubDate>
      <guid>ep-1</guid>
      <itunes:duration>1:23:45</itunes:duration>
      <enclosure url="https://example.com/ep1.mp3" length="1024" type="audio/mpeg" />
      <itunes:image href="https://example.com/ep1.jpg" />
      <podcast:transcript url="https://example.com/ep1.vtt" type="text/vtt" />
      <podcast:transcript url="https://example.com/ep1.json" type="application/json" />
      <podcast:chapters url="https://example.com/ep1-chapters.json" />
      <podcast:person role="host" group="cast" img="https://example.com/host.jpg" href="https://example.com/host">Alice Host</podcast:person>
      <podcast:soundbite startTime="60.0" duration="30.0">Punchline</podcast:soundbite>
    </item>
    <item>
      <title>Episode 2</title>
      <description>Plain description.</description>
      <content:encoded><![CDATA[<p>Rich description.</p>]]></content:encoded>
      <pubDate>Tue, 02 Jan 2024 12:00:00 +0000</pubDate>
      <enclosure url="https://example.com/ep2.mp3" type="audio/mpeg" />
    </item>
    <item>
      <title>Item without enclosure (skipped)</title>
      <description>No audio.</description>
      <pubDate>Wed, 03 Jan 2024 12:00:00 +0000</pubDate>
    </item>
  </channel>
</rss>
"#;

fn feed_url() -> Url {
    Url::parse("https://example.com/feed.xml").unwrap()
}

#[test]
fn parses_channel_metadata() {
    let parsed = parse_feed(MINIMAL_FEED.as_bytes(), &feed_url(), PodcastId::generate()).unwrap();
    assert_eq!(parsed.podcast.title, "Test Cast");
    assert_eq!(parsed.podcast.author, "Test Host");
    assert_eq!(parsed.podcast.description, "A small show for testing.");
    assert_eq!(parsed.podcast.language.as_deref(), Some("en-us"));
    assert_eq!(
        parsed.podcast.image_url.as_ref().unwrap().as_str(),
        "https://example.com/cover.jpg"
    );
    assert_eq!(parsed.podcast.categories, vec!["Technology", "News"]);
}

#[test]
fn parses_episodes_and_skips_items_without_enclosure() {
    let parsed = parse_feed(MINIMAL_FEED.as_bytes(), &feed_url(), PodcastId::generate()).unwrap();
    assert_eq!(parsed.episodes.len(), 2);

    let ep1 = &parsed.episodes[0];
    assert_eq!(ep1.title, "Episode 1");
    assert_eq!(ep1.guid, "ep-1");
    assert_eq!(ep1.duration_secs, Some(1.0 * 3600.0 + 23.0 * 60.0 + 45.0));
    assert_eq!(ep1.enclosure_url.as_str(), "https://example.com/ep1.mp3");
    assert_eq!(ep1.enclosure_mime_type.as_deref(), Some("audio/mpeg"));
    assert_eq!(
        ep1.image_url.as_ref().unwrap().as_str(),
        "https://example.com/ep1.jpg"
    );
}

#[test]
fn picks_highest_ranked_transcript() {
    let parsed = parse_feed(MINIMAL_FEED.as_bytes(), &feed_url(), PodcastId::generate()).unwrap();
    let ep1 = &parsed.episodes[0];
    // JSON outranks VTT — order in feed shouldn't matter, replace-if-better.
    assert_eq!(
        ep1.publisher_transcript_url.as_ref().unwrap().as_str(),
        "https://example.com/ep1.json"
    );
    assert_eq!(ep1.publisher_transcript_type, Some(TranscriptKind::Json));
}

#[test]
fn captures_chapters_url() {
    let parsed = parse_feed(MINIMAL_FEED.as_bytes(), &feed_url(), PodcastId::generate()).unwrap();
    assert_eq!(
        parsed.episodes[0].chapters_url.as_ref().unwrap().as_str(),
        "https://example.com/ep1-chapters.json"
    );
}

#[test]
fn captures_persons_and_soundbites() {
    let parsed = parse_feed(MINIMAL_FEED.as_bytes(), &feed_url(), PodcastId::generate()).unwrap();
    let ep1 = &parsed.episodes[0];
    let persons = ep1.persons.as_ref().expect("persons present");
    assert_eq!(persons.len(), 1);
    assert_eq!(persons[0].name, "Alice Host");
    assert_eq!(persons[0].role.as_deref(), Some("host"));
    assert_eq!(persons[0].group.as_deref(), Some("cast"));

    let bites = ep1.sound_bites.as_ref().expect("soundbites present");
    assert_eq!(bites.len(), 1);
    assert_eq!(bites[0].start_secs, 60.0);
    assert_eq!(bites[0].duration_secs, 30.0);
    assert_eq!(bites[0].title.as_deref(), Some("Punchline"));
}

#[test]
fn content_encoded_overrides_description() {
    let parsed = parse_feed(MINIMAL_FEED.as_bytes(), &feed_url(), PodcastId::generate()).unwrap();
    let ep2 = &parsed.episodes[1];
    assert!(
        ep2.description.contains("Rich description"),
        "content:encoded should overwrite the plain <description>, got: {}",
        ep2.description
    );
}

#[test]
fn missing_guid_gets_synthesized() {
    let xml = r#"<?xml version="1.0"?>
<rss version="2.0">
  <channel>
    <title>NoGuid</title>
    <description>x</description>
    <item>
      <title>E</title>
      <pubDate>Mon, 01 Jan 2024 12:00:00 +0000</pubDate>
      <enclosure url="https://example.com/a.mp3" type="audio/mpeg" />
    </item>
  </channel>
</rss>"#;
    let parsed = parse_feed(xml.as_bytes(), &feed_url(), PodcastId::generate()).unwrap();
    assert_eq!(parsed.episodes.len(), 1);
    assert!(
        parsed.episodes[0].guid.starts_with("synth::"),
        "expected synth guid, got: {}",
        parsed.episodes[0].guid
    );
}

#[test]
fn missing_channel_errors() {
    let xml = "<?xml version=\"1.0\"?><rss version=\"2.0\"></rss>";
    let err = parse_feed(xml.as_bytes(), &feed_url(), PodcastId::generate()).unwrap_err();
    assert!(matches!(err, podcast_feeds::ParseError::MissingChannel));
}

#[test]
fn malformed_xml_errors() {
    // Mismatched end tag — `check_end_names` will catch this.
    let xml = "<rss><channel><title>X</titl></channel></rss>";
    let result = parse_feed(xml.as_bytes(), &feed_url(), PodcastId::generate());
    assert!(
        matches!(result, Err(podcast_feeds::ParseError::MalformedXml(_))),
        "expected MalformedXml, got {result:?}"
    );
}

#[test]
fn malformed_pub_date_falls_back_to_epoch() {
    let xml = r#"<?xml version="1.0"?>
<rss version="2.0">
  <channel>
    <title>X</title>
    <description>y</description>
    <item>
      <title>E</title>
      <pubDate>not a real date</pubDate>
      <guid>g1</guid>
      <enclosure url="https://example.com/a.mp3" type="audio/mpeg" />
    </item>
  </channel>
</rss>"#;
    let parsed = parse_feed(xml.as_bytes(), &feed_url(), PodcastId::generate()).unwrap();
    assert_eq!(parsed.episodes[0].pub_date.timestamp(), 0);
}
