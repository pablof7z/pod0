use podcast_core::{NostrVisibility, Podcast, PodcastId};
use podcast_feeds::opml::{export::export_opml_with, import_opml};
use url::Url;

use chrono::{TimeZone, Utc};

fn fixture_podcast(title: &str, url: &str, description: &str, language: Option<&str>) -> Podcast {
    Podcast {
        id: PodcastId::generate(),
        feed_url: Some(Url::parse(url).unwrap()),
        title: title.to_string(),
        author: String::new(),
        image_url: None,
        description: description.to_string(),
        language: language.map(String::from),
        categories: Vec::new(),
        discovered_at: Utc::now(),
        owner_pubkey_hex: None,
        nostr_visibility: NostrVisibility::Public,
        nostr_coordinate: None,
        title_is_placeholder: false,
        last_refreshed_at: None,
        etag: None,
        last_modified: None,
    }
}

#[test]
fn export_then_import_preserves_feed_urls_and_titles() {
    let originals = vec![
        fixture_podcast(
            "Tim Ferriss",
            "https://feeds.example/timferriss.rss",
            "Long-form interviews.",
            Some("en"),
        ),
        fixture_podcast("NPR Up First", "https://feeds.example/upfirst.rss", "", None),
        fixture_podcast(
            "Daring Fireball",
            "https://daringfireball.net/feeds/main",
            "Linked list.",
            Some("en"),
        ),
    ];

    let xml = export_opml_with(
        &originals,
        "Podcastr Subscriptions",
        Utc.with_ymd_and_hms(2026, 1, 1, 12, 0, 0).unwrap(),
    );

    let reimported = import_opml(&xml).unwrap();

    assert_eq!(reimported.len(), originals.len());
    for (orig, reim) in originals.iter().zip(reimported.iter()) {
        assert_eq!(orig.feed_url, reim.feed_url);
        assert_eq!(orig.title, reim.title);
        assert_eq!(orig.description, reim.description);
        assert_eq!(orig.language, reim.language);
    }
}

#[test]
fn import_skips_outline_without_xml_url() {
    let xml = r#"<?xml version="1.0"?>
<opml version="2.0">
  <body>
    <outline text="Folder">
      <outline type="rss" text="Real Feed" xmlUrl="https://example.com/feed.xml" />
    </outline>
  </body>
</opml>"#;
    let podcasts = import_opml(xml).unwrap();
    assert_eq!(podcasts.len(), 1);
    assert_eq!(podcasts[0].title, "Real Feed");
    assert_eq!(
        podcasts[0].feed_url.as_ref().unwrap().as_str(),
        "https://example.com/feed.xml"
    );
}

#[test]
fn import_dedupes_repeated_feed_urls() {
    let xml = r#"<?xml version="1.0"?>
<opml version="2.0">
  <body>
    <outline type="rss" text="A" xmlUrl="https://example.com/feed.xml" />
    <outline type="rss" text="A again" xmlUrl="https://example.com/feed.xml" />
    <outline type="rss" text="B" xmlUrl="https://example.com/other.xml" />
  </body>
</opml>"#;
    let podcasts = import_opml(xml).unwrap();
    assert_eq!(podcasts.len(), 2);
    assert_eq!(podcasts[0].title, "A");
    assert_eq!(podcasts[1].title, "B");
}

#[test]
fn import_falls_back_to_title_then_host_for_display_name() {
    let xml = r#"<?xml version="1.0"?>
<opml version="2.0">
  <body>
    <outline type="rss" title="Title Attr" xmlUrl="https://feeds.example/a.xml" />
    <outline type="rss" xmlUrl="https://feeds.example/b.xml" />
  </body>
</opml>"#;
    let podcasts = import_opml(xml).unwrap();
    assert_eq!(podcasts[0].title, "Title Attr");
    assert_eq!(podcasts[1].title, "feeds.example");
}

#[test]
fn import_malformed_errors() {
    let result = import_opml("<opml><body><outline></body>");
    assert!(matches!(
        result,
        Err(podcast_feeds::OpmlError::MalformedXml(_))
    ));
}

#[test]
fn export_skips_synthetic_podcasts_without_feed_url() {
    let mut synthetic = fixture_podcast("X", "https://example.com/x.xml", "", None);
    synthetic.feed_url = None;
    let real = fixture_podcast("Y", "https://example.com/y.xml", "", None);
    let xml = export_opml_with(
        &[synthetic, real],
        "T",
        Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
    );
    // Only the real podcast should appear in the OPML.
    assert!(xml.contains("https://example.com/y.xml"));
    assert!(!xml.contains("\"X\""));
}

#[test]
fn export_escapes_xml_attribute_values() {
    let podcast = fixture_podcast(
        "Q&A < 30 > \"daily\"",
        "https://example.com/q.xml",
        "It's & it ain't.",
        None,
    );
    let xml = export_opml_with(
        &[podcast],
        "T",
        Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
    );
    assert!(xml.contains("&amp;"));
    assert!(xml.contains("&lt;"));
    assert!(xml.contains("&gt;"));
    assert!(xml.contains("&quot;"));
    assert!(xml.contains("&apos;"));
}
