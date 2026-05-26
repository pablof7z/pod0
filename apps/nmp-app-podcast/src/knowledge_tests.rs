//! Tests for [`super::knowledge`] — knowledge-search matching, ranking, and snapshot projection.
//!
//! Extracted from `knowledge.rs` to keep that file under the 500-line hard limit.

use super::*;
use podcast_core::{Episode, Podcast, PodcastId};
use url::Url;
use uuid::Uuid;

fn make_episode(podcast_id: PodcastId, title: &str, description: &str) -> Episode {
    let mut ep = Episode::new(
        podcast_id,
        "https://example.com/feed.xml",
        format!("guid-{}", Uuid::new_v4()),
        title,
        Url::parse("https://example.com/audio.mp3").unwrap(),
        chrono::Utc::now(),
    );
    ep.description = description.to_owned();
    ep
}

#[test]
fn empty_query_returns_no_results() {
    let store = PodcastStore::new();
    assert!(collect_knowledge_matches(&store, "").is_empty());
    assert!(collect_knowledge_matches(&store, "   ").is_empty());
}

#[test]
fn substring_match_is_case_insensitive() {
    let mut store = PodcastStore::new();
    let podcast = Podcast::new("Tech Talk");
    let id = podcast.id;
    let ep = make_episode(id, "Episode 1", "We discuss MACHINE learning techniques.");
    store.subscribe(podcast, vec![ep.clone()]);

    let results = collect_knowledge_matches(&store, "machine");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].episode_id, ep.id.0.to_string());
    assert_eq!(results[0].podcast_title, "Tech Talk");
    assert!(results[0].relevance_score > 0.0);
}

#[test]
fn returns_at_most_top_k_results() {
    let mut store = PodcastStore::new();
    let podcast = Podcast::new("Show");
    let id = podcast.id;
    let episodes: Vec<Episode> = (0..15)
        .map(|i| make_episode(id, &format!("nostr episode {i}"), "about nostr"))
        .collect();
    store.subscribe(podcast, episodes);

    let results = collect_knowledge_matches(&store, "nostr");
    assert_eq!(results.len(), KNOWLEDGE_SEARCH_TOP_K);
}

#[test]
fn title_match_outranks_description_match() {
    let mut store = PodcastStore::new();
    let podcast = Podcast::new("Show");
    let id = podcast.id;
    // Episode A: needle in description only.
    let ep_a = make_episode(id, "Random title", "deep dive on nostr relays");
    // Episode B: needle in title.
    let ep_b = make_episode(id, "nostr fundamentals", "intro chat");
    store.subscribe(podcast, vec![ep_a.clone(), ep_b.clone()]);

    let results = collect_knowledge_matches(&store, "nostr");
    assert_eq!(results.len(), 2);
    // Title-match (ep_b) must outrank description-match (ep_a).
    assert_eq!(results[0].episode_id, ep_b.id.0.to_string());
}

#[test]
fn no_match_returns_empty() {
    let mut store = PodcastStore::new();
    let podcast = Podcast::new("Show");
    let id = podcast.id;
    let ep = make_episode(id, "About cats", "feline behavior research");
    store.subscribe(podcast, vec![ep]);

    let results = collect_knowledge_matches(&store, "quantum");
    assert!(results.is_empty());
}

#[test]
fn snippet_truncates_long_text_with_ellipsis() {
    let long = "a".repeat(500);
    let body = format!("{}MATCH{}", long, long);
    let snippet = build_snippet(&body, long.len(), "MATCH".len());
    assert!(snippet.chars().count() <= KNOWLEDGE_SNIPPET_MAX_CHARS + 2);
    assert!(snippet.contains("MATCH"));
    assert!(snippet.starts_with('…'));
    assert!(snippet.ends_with('…'));
}

#[test]
fn snippet_passes_through_short_text_unchanged() {
    let body = "Short description with a match here.";
    let pos = body.find("match").unwrap();
    let snippet = build_snippet(body, pos, "match".len());
    assert_eq!(snippet, body);
}

#[test]
fn snippet_safe_on_multibyte_utf8() {
    // Em-dashes and other multi-byte chars must not panic the slicer.
    let prefix: String = std::iter::repeat("ä").take(300).collect();
    let body = format!("{prefix}NEEDLE{prefix}");
    let pos = body.find("NEEDLE").unwrap();
    let snippet = build_snippet(&body, pos, "NEEDLE".len());
    assert!(snippet.contains("NEEDLE"));
}

#[test]
fn snapshot_round_trips_knowledge_search_results() {
    use crate::ffi::PodcastUpdate;
    let row = KnowledgeSearchResult {
        episode_id: "ep-1".into(),
        episode_title: "Pilot".into(),
        podcast_title: "Some Show".into(),
        snippet: "the exact text fragment".into(),
        start_secs: Some(42.0),
        relevance_score: 0.93,
    };
    let snap = PodcastUpdate {
        knowledge_search_results: vec![row.clone()],
        ..PodcastUpdate::default()
    };
    let json = serde_json::to_string(&snap).expect("encode");
    assert!(json.contains("knowledge_search_results"));
    let decoded: PodcastUpdate = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded.knowledge_search_results, vec![row]);
}

#[test]
fn snapshot_omits_empty_knowledge_search_results() {
    // D5 byte-identity: an empty knowledge_search_results array must
    // not bloat the wire payload (preserves the legacy stub shape).
    use crate::ffi::PodcastUpdate;
    let json = serde_json::to_string(&PodcastUpdate::default()).expect("encode");
    assert!(!json.contains("knowledge_search_results"));
}

#[test]
fn relevance_score_is_bounded() {
    let mut store = PodcastStore::new();
    let podcast = Podcast::new("Show");
    let id = podcast.id;
    let ep = make_episode(id, "x", "x");
    store.subscribe(podcast, vec![ep]);

    let results = collect_knowledge_matches(&store, "x");
    assert_eq!(results.len(), 1);
    assert!(results[0].relevance_score >= 0.0);
    assert!(results[0].relevance_score <= 1.0);
}
