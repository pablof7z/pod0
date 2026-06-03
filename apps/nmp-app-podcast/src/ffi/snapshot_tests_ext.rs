//! Continuation of snapshot round-trip tests (part 2/2).
//!
//! Split from `snapshot_tests.rs` to keep both files under the 500-line
//! AGENTS.md hard limit.

use crate::ffi::projections::{
    AgentPickSummary, ClipSummary, CommentSummary,
    InboxItem, MemoryFact, WikiArticle,
};
use super::PodcastUpdate;

#[test]
fn snapshot_with_comments_round_trips() {
    let comments = vec![
        CommentSummary {
            id: "a".repeat(64),
            author_npub: "npub1example".into(),
            author_name: Some("Satoshi".into()),
            content: "Great episode!".into(),
            created_at: 1_700_000_100,
        },
        CommentSummary {
            id: "b".repeat(64),
            author_npub: "npub1other".into(),
            author_name: None,
            content: "Agreed.".into(),
            created_at: 1_700_000_050,
        },
    ];
    let snap = PodcastUpdate {
        comments: comments.clone(),
        ..PodcastUpdate::default()
    };
    let json = serde_json::to_string(&snap).expect("encode");
    let decoded: PodcastUpdate = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded.comments, comments);
}

#[test]
fn default_snapshot_omits_empty_comments() {
    let json = serde_json::to_string(&PodcastUpdate::default()).expect("encode");
    assert!(!json.contains("\"comments\""));
}

// ── Queue projection (M12 / PR 12) ───────────────────────────────

#[test]
fn empty_queue_is_omitted_from_wire_payload() {
    // D5 byte-identity: an empty queue must not bloat the snapshot.
    let json = serde_json::to_string(&PodcastUpdate::default()).expect("encode");
    assert!(!json.contains("queue"));
}

#[test]
fn snapshot_with_queue_round_trips() {
    use crate::ffi::projections::EpisodeSummary;
    let ep = EpisodeSummary { id: "ep-1".into(), title: "Episode 1".into(), ..EpisodeSummary::default() };
    let snap = PodcastUpdate {
        queue: vec![ep.clone()],
        ..PodcastUpdate::default()
    };
    let json = serde_json::to_string(&snap).expect("encode");
    assert!(json.contains("\"queue\":["));
    let decoded: PodcastUpdate = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded.queue, vec![ep]);
}

// ── Wiki article snapshot wiring (#39 — AI wiki scaffold) ────────────────

#[test]
fn snapshot_with_wiki_articles_round_trips() {
    let snap = PodcastUpdate {
        wiki_articles: vec![WikiArticle {
            id: "art-1".into(),
            podcast_id: "pod-1".into(),
            topic: "Halving cycles".into(),
            summary: "Summary body.".into(),
            source_episode_ids: vec!["ep-1".into()],
            last_updated_at: 1_700_000_000,
            is_generating: false,
            generation_error: None,
        }],
        ..PodcastUpdate::default()
    };
    let json = serde_json::to_string(&snap).expect("encode");
    assert!(json.contains(r#""wiki_articles""#));
    let decoded: PodcastUpdate = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded.wiki_articles, snap.wiki_articles);
}

#[test]
fn snapshot_omits_empty_wiki_articles() {
    // D5 byte-identity: empty wiki list must not bloat the wire payload.
    let json = serde_json::to_string(&PodcastUpdate::default()).expect("encode");
    assert!(!json.contains("wiki_articles"));
    assert!(!json.contains("wiki_search_results"));
}

#[test]
fn snapshot_with_wiki_search_results_round_trips() {
    let snap = PodcastUpdate {
        wiki_search_results: vec![WikiArticle {
            id: "art-2".into(),
            podcast_id: "pod-1".into(),
            topic: "Lightning routing".into(),
            summary: "Summary.".into(),
            source_episode_ids: vec![],
            last_updated_at: 1_700_000_100,
            is_generating: false,
            generation_error: None,
        }],
        ..PodcastUpdate::default()
    };
    let json = serde_json::to_string(&snap).expect("encode");
    assert!(json.contains(r#""wiki_search_results""#));
    let decoded: PodcastUpdate = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded.wiki_search_results, snap.wiki_search_results);
}

// ── AgentPickSummary snapshot wiring (feature #46) ───────────────

#[test]
fn snapshot_picks_round_trips_and_default_omits_field() {
    let json = serde_json::to_string(&PodcastUpdate::default()).expect("encode");
    assert!(!json.contains("picks"));
    let pick = AgentPickSummary {
        episode_id: "ep-1".into(),
        episode_title: "Pilot".into(),
        podcast_id: "pod-1".into(),
        podcast_title: "Show".into(),
        published_at: 1_700_000_000,
        pick_reason: "New from Show".into(),
        pick_score: 1.0,
        ..AgentPickSummary::default()
    };
    let snap = PodcastUpdate { picks: vec![pick.clone()], ..PodcastUpdate::default() };
    let decoded: PodcastUpdate =
        serde_json::from_str(&serde_json::to_string(&snap).expect("encode"))
            .expect("decode");
    assert_eq!(decoded.picks, vec![pick]);
}

// ── Agent memory (feature #33) ───────────────────────────────────

#[test]
fn snapshot_omits_empty_memory_facts() {
    let json = serde_json::to_string(&PodcastUpdate::default()).expect("encode");
    assert!(!json.contains("memory_facts"));
}

#[test]
fn snapshot_with_memory_facts_round_trips() {
    let facts = vec![
        MemoryFact {
            id: "k1".into(),
            key: "k1".into(),
            value: "v1".into(),
            source: "user".into(),
            created_at: 1_700_000_000,
        },
        MemoryFact {
            id: "k2".into(),
            key: "k2".into(),
            value: "v2".into(),
            source: "agent".into(),
            created_at: 1_700_000_500,
        },
    ];
    let snap = PodcastUpdate {
        memory_facts: facts.clone(),
        ..PodcastUpdate::default()
    };
    let json = serde_json::to_string(&snap).expect("encode");
    let decoded: PodcastUpdate = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded.memory_facts, facts);
}

#[test]
fn snapshot_with_clips_round_trips() {
    let clip = ClipSummary {
        id: "clip-1".into(),
        episode_id: "ep-1".into(),
        episode_title: "Pilot".into(),
        podcast_title: "Some Show".into(),
        start_secs: 10.0,
        end_secs: 70.0,
        title: Some("Marcus on retrieval".into()),
        created_at: 1_700_000_000,
    };
    let snap = PodcastUpdate {
        clips: vec![clip.clone()],
        ..PodcastUpdate::default()
    };
    let json = serde_json::to_string(&snap).expect("encode");
    assert!(json.contains("\"clips\":["));
    let decoded: PodcastUpdate = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded.clips, vec![clip]);
}

#[test]
fn default_snapshot_omits_empty_clips() {
    let json = serde_json::to_string(&PodcastUpdate::default()).expect("encode");
    assert!(!json.contains("clips"));
}

#[test]
fn snapshot_with_inbox_round_trips_and_empty_is_omitted() {
    let empty_json = serde_json::to_string(&PodcastUpdate::default()).expect("encode");
    assert!(!empty_json.contains("inbox"));

    let item = InboxItem {
        episode_id: "ep-1".into(),
        episode_title: "Pilot".into(),
        podcast_id: "pod-1".into(),
        podcast_title: "Some Show".into(),
        artwork_url: None,
        published_at: 1_700_000_000,
        duration_secs: None,
        priority_score: 0.9,
        priority_reason: Some("Just published".into()),
        ai_categories: vec![],
    };
    let snap = PodcastUpdate {
        inbox: vec![item.clone()],
        ..PodcastUpdate::default()
    };
    let json = serde_json::to_string(&snap).expect("encode");
    assert!(json.contains(r#""inbox":["#));
    let decoded: PodcastUpdate = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded.inbox, vec![item]);
}
