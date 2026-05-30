//! Tests for the agent-chat tool layer.
//!
//! Covers the three podcast-domain tools (`search_library`, `get_transcript`,
//! `get_podcast_info`) plus the JSON tool-call parser that drives the manual
//! tool-calling loop in [`crate::agent_llm::chat_with_tools`].

use std::sync::{Arc, Mutex};

use chrono::{TimeZone, Utc};
use podcast_core::{Episode, Podcast};
use url::Url;

use super::*;
use crate::store::PodcastStore;

/// Build a fixture store with one podcast and one episode.
/// Returns `(store, podcast_id_str, episode_id_str)`.
fn fixture_store() -> (Arc<Mutex<PodcastStore>>, String, String) {
    let mut store = PodcastStore::new();

    let mut podcast = Podcast::new("Bitcoin Weekly");
    podcast.author = "Satoshi".to_owned();
    let feed_url = "https://example.com/feed.xml";
    podcast.feed_url = Some(Url::parse(feed_url).unwrap());
    let podcast_id_str = podcast.id.0.to_string();

    let episode = Episode::new(
        podcast.id,
        feed_url,
        "guid-1",
        "Understanding Bitcoin Halving",
        Url::parse("https://example.com/ep1.mp3").unwrap(),
        Utc.with_ymd_and_hms(2026, 5, 1, 12, 0, 0).unwrap(),
    );
    let episode_id_str = episode.id.0.to_string();

    store.subscribe(podcast, vec![episode]);

    (Arc::new(Mutex::new(store)), podcast_id_str, episode_id_str)
}

#[test]
fn search_library_returns_results_for_known_title() {
    let (store, _pid, _eid) = fixture_store();
    let registry = ToolRegistry::new(store);

    let out = registry.execute("search_library", &serde_json::json!({ "query": "bitcoin" }));

    assert!(
        out.to_lowercase().contains("bitcoin"),
        "search result should mention the matching episode/podcast, got: {out}"
    );
    assert!(
        out.contains("Understanding Bitcoin Halving"),
        "search should surface the matching episode title, got: {out}"
    );
}

#[test]
fn search_library_reports_no_matches_for_unknown_query() {
    let (store, _pid, _eid) = fixture_store();
    let registry = ToolRegistry::new(store);

    let out = registry.execute("search_library", &serde_json::json!({ "query": "zzzznomatch" }));

    assert!(
        out.to_lowercase().contains("no match") || out.to_lowercase().contains("no result"),
        "expected a no-match message, got: {out}"
    );
}

#[test]
fn get_transcript_returns_stored_text() {
    let (store, _pid, eid) = fixture_store();
    {
        let mut s = store.lock().unwrap();
        s.set_transcript(eid.clone(), "the quick brown fox transcript".to_owned());
    }
    let registry = ToolRegistry::new(store);

    let out = registry.execute("get_transcript", &serde_json::json!({ "episode_id": eid }));

    assert!(
        out.contains("the quick brown fox transcript"),
        "get_transcript should return the stored text, got: {out}"
    );
}

#[test]
fn get_transcript_reports_when_missing() {
    let (store, _pid, eid) = fixture_store();
    let registry = ToolRegistry::new(store);

    let out = registry.execute("get_transcript", &serde_json::json!({ "episode_id": eid }));

    assert!(
        out.to_lowercase().contains("no transcript"),
        "expected a no-transcript message, got: {out}"
    );
}

#[test]
fn get_transcript_truncates_to_2000_chars() {
    let (store, _pid, eid) = fixture_store();
    let long = "x".repeat(5000);
    {
        let mut s = store.lock().unwrap();
        s.set_transcript(eid.clone(), long);
    }
    let registry = ToolRegistry::new(store);

    let out = registry.execute("get_transcript", &serde_json::json!({ "episode_id": eid }));
    let x_count = out.chars().filter(|c| *c == 'x').count();
    assert!(x_count <= 2000, "transcript must be truncated to <= 2000 chars, got {x_count}");
}

#[test]
fn get_podcast_info_returns_title_and_count() {
    let (store, pid, _eid) = fixture_store();
    let registry = ToolRegistry::new(store);

    let out = registry.execute("get_podcast_info", &serde_json::json!({ "podcast_id": pid }));

    assert!(out.contains("Bitcoin Weekly"), "should include podcast title, got: {out}");
    assert!(out.contains('1'), "should include episode count (1), got: {out}");
}

#[test]
fn get_memory_facts_lists_stored_facts() {
    let (store, _pid, _eid) = fixture_store();
    {
        let mut s = store.lock().unwrap();
        s.set_memory_fact("preferred_genre".into(), "true crime".into(), "user".into(), 1);
    }
    let registry = ToolRegistry::new(store);

    let out = registry.execute("get_memory_facts", &serde_json::json!({}));

    assert!(out.contains("preferred_genre"), "should include the fact key, got: {out}");
    assert!(out.contains("true crime"), "should include the fact value, got: {out}");
}

#[test]
fn get_memory_facts_reports_when_empty() {
    let (store, _pid, _eid) = fixture_store();
    let registry = ToolRegistry::new(store);

    let out = registry.execute("get_memory_facts", &serde_json::json!({}));

    assert!(
        out.to_lowercase().contains("no memory facts"),
        "expected an empty-memory message, got: {out}"
    );
}

#[test]
fn unknown_tool_returns_error_string() {
    let (store, _pid, _eid) = fixture_store();
    let registry = ToolRegistry::new(store);

    let out = registry.execute("frobnicate", &serde_json::json!({}));
    assert!(
        out.to_lowercase().contains("unknown tool"),
        "expected unknown-tool error, got: {out}"
    );
}

#[test]
fn tool_call_json_is_detected() {
    let raw = r#"{"tool":"search_library","args":{"query":"test"}}"#;
    let call = parse_tool_call(raw).expect("should parse a tool call");
    assert_eq!(call.name, "search_library");
    assert_eq!(call.args["query"], "test");
}

#[test]
fn tool_call_json_detected_with_surrounding_text() {
    // Local models often wrap JSON in prose or code fences.
    let raw = "Sure, let me look that up.\n```json\n{\"tool\":\"get_transcript\",\"args\":{\"episode_id\":\"abc\"}}\n```";
    let call = parse_tool_call(raw).expect("should parse a tool call embedded in text");
    assert_eq!(call.name, "get_transcript");
    assert_eq!(call.args["episode_id"], "abc");
}

#[test]
fn plain_text_response_is_not_a_tool_call() {
    let raw = "Bitcoin is a decentralized digital currency.";
    assert!(parse_tool_call(raw).is_none(), "plain prose must not parse as a tool call");
}
