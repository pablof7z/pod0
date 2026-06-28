//! Continuation of snapshot round-trip tests (part 2/2).
//!
//! Split from `snapshot_tests.rs` to keep both files under the 500-line
//! AGENTS.md hard limit.

use super::PodcastUpdate;
use crate::ffi::projections::{
    AgentPickSummary, ClipSummary, CommentSummary, FriendSummary, InboxItem, MemoryFact,
};

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
    let ep = EpisodeSummary {
        id: "ep-1".into(),
        title: "Episode 1".into(),
        ..EpisodeSummary::default()
    };
    let snap = PodcastUpdate {
        queue: vec![ep.clone()],
        ..PodcastUpdate::default()
    };
    let json = serde_json::to_string(&snap).expect("encode");
    assert!(json.contains("\"queue\":["));
    let decoded: PodcastUpdate = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded.queue, vec![ep]);
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
    let snap = PodcastUpdate {
        picks: vec![pick.clone()],
        ..PodcastUpdate::default()
    };
    let decoded: PodcastUpdate =
        serde_json::from_str(&serde_json::to_string(&snap).expect("encode")).expect("decode");
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
        transcript_text: "A useful quote.".into(),
        speaker: Some("spk_0".into()),
        source: "auto".into(),
        refinement_status: "transcript_refined".into(),
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
fn snapshot_with_friends_round_trips_and_empty_is_omitted() {
    let empty_json = serde_json::to_string(&PodcastUpdate::default()).expect("encode");
    assert!(!empty_json.contains("friends"));

    let friend = FriendSummary {
        id: "friend-1".into(),
        display_name: "Alice".into(),
        pubkey_hex: "aabbcc".into(),
        added_at: 123,
        avatar_url: Some("https://example.com/alice.png".into()),
        about: Some("Builds shows".into()),
    };
    let snap = PodcastUpdate {
        friends: vec![friend.clone()],
        ..PodcastUpdate::default()
    };
    let json = serde_json::to_string(&snap).expect("encode");
    assert!(json.contains(r#""friends":["#));
    assert!(json.contains(r#""pubkey_hex":"aabbcc""#));
    let decoded: PodcastUpdate = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded.friends, vec![friend]);
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

#[test]
fn snapshot_with_inbox_last_triaged_at_round_trips_and_none_is_omitted() {
    let empty_json = serde_json::to_string(&PodcastUpdate::default()).expect("encode");
    assert!(!empty_json.contains("inbox_last_triaged_at"));

    let snap = PodcastUpdate {
        inbox_last_triaged_at: Some(1_717_200_123),
        ..PodcastUpdate::default()
    };
    let json = serde_json::to_string(&snap).expect("encode");
    assert!(json.contains(r#""inbox_last_triaged_at":1717200123"#));

    let decoded: PodcastUpdate = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded.inbox_last_triaged_at, Some(1_717_200_123));
}
