use super::*;

#[test]
fn parses_overcast_metadata_and_entities() {
    let html = br#"<link rel="canonical" href="https://example.com/42">
      <meta property="og:title" content="Episode &mdash; Test Show">
      <meta property="og:description" content="Useful &amp; clear.">
      <meta name="twitter:player:stream" content="https://cdn.test/42.mp3#t=0">
      <a href="https://podcasts.apple.com/podcast/id123456789">Apple</a>"#;
    let value = parse_episode_web_page(html, "https://overcast.fm/+abc").unwrap();
    assert_eq!(value.episode_title.as_deref(), Some("Episode"));
    assert_eq!(value.podcast_title.as_deref(), Some("Test Show"));
    assert_eq!(value.description.as_deref(), Some("Useful & clear."));
    assert_eq!(value.apple_podcast_id.as_deref(), Some("123456789"));
}

#[test]
fn parses_structured_episode_and_embedded_apple_fields() {
    let html = br#"<script type="application/ld+json">{
      "@type":"PodcastEpisode","name":"The Episode","datePublished":"2026-07-26T12:30:00Z",
      "duration":"PT1H2M3S","partOfSeries":{"name":"The Show"}}
      </script><script>{"contentId":"1000","feedUrl":"https:\/\/example.com\/feed.xml",
      "guid":"guid","streamUrl":"https:\/\/cdn.example.com\/episode.m4a"}</script>"#;
    let value = parse_episode_web_page(
        html,
        "https://podcasts.apple.com/us/podcast/show/id987654321?i=1000",
    )
    .unwrap();
    assert_eq!(value.duration_milliseconds, Some(3_723_000));
    assert_eq!(
        value.feed_url.as_deref(),
        Some("https://example.com/feed.xml")
    );
    assert_eq!(
        value.audio_url.as_deref(),
        Some("https://cdn.example.com/episode.m4a")
    );
    assert_eq!(value.guid.as_deref(), Some("guid"));
}

#[test]
fn resolves_relative_feed_and_audio_links() {
    let html = br#"<link type='application/rss+xml' rel='alternate' href='/podcast.xml'>
      <meta property='og:title' content='Shared Episode'><audio src='/media/shared.mp3'>"#;
    let value = parse_episode_web_page(html, "https://publisher.example/episodes/shared").unwrap();
    assert_eq!(
        value.feed_url.as_deref(),
        Some("https://publisher.example/podcast.xml")
    );
    assert_eq!(
        value.audio_url.as_deref(),
        Some("https://publisher.example/media/shared.mp3")
    );
}
