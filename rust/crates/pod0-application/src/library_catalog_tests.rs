use super::*;

#[test]
fn catalog_ranking_prefers_title_phrase_and_show_hint() {
    let xml = br#"<rss><channel><title>Science Weekly</title><author>Lab</author>
      <item><guid>a</guid><title>Black Holes Explained</title><description>space</description>
        <pubDate>Tue, 10 Jun 2025 10:00:00 GMT</pubDate>
        <enclosure url="https://cdn.example/a.mp3" type="audio/mpeg"/></item>
      <item><guid>b</guid><title>Weekly News</title><description>black holes mentioned briefly</description>
        <pubDate>Tue, 11 Jun 2025 10:00:00 GMT</pubDate>
        <enclosure url="https://cdn.example/b.mp3" type="audio/mpeg"/></item>
    </channel></rss>"#;
    let values = catalog_candidates_from_feed(
        xml,
        "https://feeds.example/science.xml",
        "black holes",
        Some("Science Weekly"),
        1_750_000_000_000,
    )
    .unwrap();
    let selected = select_catalog_candidates(values, 1);
    assert_eq!(selected.len(), 1);
    assert_eq!(selected[0].episode.title, "Black Holes Explained");
    assert!(selected[0].score >= 200);
}

#[test]
fn catalog_selection_is_bounded_and_deterministic() {
    let candidate = |title: &str, score: u32, published: i64| CatalogEpisodeCandidate {
        episode: ResolvedSharedEpisode {
            podcast_id: pod0_domain::PodcastId::from_parts(1, score.into()),
            podcast_title: "Show".into(),
            feed_url: Some("https://example.com/feed".into()),
            audio_url: format!("https://example.com/{score}.mp3"),
            guid: Some(score.to_string()),
            title: title.into(),
            description: String::new(),
            published_at_milliseconds: published,
            enclosure_mime_type: Some("audio/mpeg".into()),
            image_url: None,
            duration_milliseconds: None,
        },
        score,
    };
    let selected = select_catalog_candidates(
        vec![
            candidate("older", 10, 1),
            candidate("newer", 10, 2),
            candidate("best", 20, 0),
        ],
        2,
    );
    assert_eq!(
        selected
            .iter()
            .map(|value| value.episode.title.as_str())
            .collect::<Vec<_>>(),
        vec!["best", "newer"]
    );
}
