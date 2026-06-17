//! Continuation of round-trip tests for [`super::projections`] (part 2/2).
//!
//! Split from `projections_tests.rs` to keep both files under the 500-line
//! AGENTS.md hard limit.

use super::projections::{
    AgentPickSummary, AgentTaskSummary, CategoryBrowseItem, ChapterSummary, ClipSummary,
    CommentSummary, EpisodeSummary, KnowledgeSearchResult, MemoryFact, SocialSnapshot,
};

#[test]
fn comment_summary_omits_none_author_name() {
    let c = CommentSummary {
        id: "abc".into(),
        author_npub: "npub1example".into(),
        author_name: None,
        content: "first!".into(),
        created_at: 1_700_000_000,
    };
    let json = serde_json::to_string(&c).expect("encode");
    assert!(!json.contains("author_name"));
    let decoded: CommentSummary = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, c);
}

#[test]
fn comment_summary_round_trips_with_author_name() {
    let c = CommentSummary {
        id: "abc".into(),
        author_npub: "npub1example".into(),
        author_name: Some("Satoshi".into()),
        content: "love this episode".into(),
        created_at: 1_700_000_000,
    };
    let json = serde_json::to_string(&c).expect("encode");
    assert!(json.contains("\"author_name\":\"Satoshi\""));
    let decoded: CommentSummary = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, c);
}

#[test]
fn chapter_summary_ai_generated_round_trip() {
    let ai = ChapterSummary {
        start_secs: 0.0,
        title: "Chapter 1".into(),
        is_ai_generated: true,
        ..ChapterSummary::default()
    };
    let json = serde_json::to_string(&ai).expect("encode");
    assert!(json.contains("\"is_ai_generated\":true"));
    let decoded: ChapterSummary = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, ai);
}

#[test]
fn chapter_summary_decodes_when_is_ai_generated_omitted() {
    let json = r#"{"start_secs":0.0,"title":"Intro"}"#;
    let decoded: ChapterSummary = serde_json::from_str(json).expect("decode");
    assert!(!decoded.is_ai_generated);
}

#[test]
fn chapter_summary_omits_publisher_source_on_wire() {
    // Publisher is the default + carries no signal → must not bloat the payload.
    let pub_chapter = ChapterSummary {
        start_secs: 0.0,
        title: "RSS chapter".into(),
        source: podcast_core::ChapterSource::Publisher,
        ..ChapterSummary::default()
    };
    let json = serde_json::to_string(&pub_chapter).expect("encode");
    assert!(
        !json.contains("source"),
        "publisher source must be skipped: {json}"
    );
}

#[test]
fn chapter_summary_serializes_llm_and_stub_source() {
    let llm = ChapterSummary {
        start_secs: 0.0,
        title: "Real topic".into(),
        source: podcast_core::ChapterSource::Llm,
        ..ChapterSummary::default()
    };
    let json = serde_json::to_string(&llm).expect("encode");
    assert!(json.contains("\"source\":\"llm\""), "got: {json}");
    assert_eq!(
        serde_json::from_str::<ChapterSummary>(&json).expect("decode"),
        llm
    );

    let stub = ChapterSummary {
        start_secs: 0.0,
        title: "Chapter 1".into(),
        source: podcast_core::ChapterSource::Stub,
        ..ChapterSummary::default()
    };
    let stub_json = serde_json::to_string(&stub).expect("encode");
    assert!(
        stub_json.contains("\"source\":\"stub\""),
        "got: {stub_json}"
    );
}

#[test]
fn chapter_summary_decodes_source_default_publisher_when_omitted() {
    // A pre-`source` snapshot (no field) must decode as Publisher for wire-compat.
    let json = r#"{"start_secs":0.0,"title":"Intro"}"#;
    let decoded: ChapterSummary = serde_json::from_str(json).expect("decode");
    assert_eq!(decoded.source, podcast_core::ChapterSource::Publisher);
}

#[test]
fn agent_task_summary_round_trips_with_all_fields() {
    let task = AgentTaskSummary {
        id: "task-1".into(),
        title: "Inbox Triage".into(),
        description: Some("Triage the inbox every morning".into()),
        intent_type: "inbox_triage".into(),
        intent_label: "Triage inbox".into(),
        intent_detail: Some("Prioritize new episodes".into()),
        action_namespace: "podcast.inbox.triage".into(),
        action_body: "{}".into(),
        schedule: "daily".into(),
        next_run_at: Some(1_700_000_000),
        last_run_at: Some(1_699_900_000),
        status: "completed".into(),
        is_enabled: true,
    };
    let json = serde_json::to_string(&task).expect("encode");
    assert!(!json.contains("action_namespace"));
    assert!(!json.contains("action_body"));
    let decoded: AgentTaskSummary = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded.intent_type, "inbox_triage");
    assert_eq!(decoded.intent_label, "Triage inbox");
    assert_eq!(decoded.action_namespace, "");
    assert_eq!(decoded.action_body, "");
}

#[test]
fn agent_task_summary_omits_none_optionals() {
    let task = AgentTaskSummary {
        id: "task-1".into(),
        title: "Inbox Triage".into(),
        description: None,
        intent_type: "inbox_triage".into(),
        intent_label: "Triage inbox".into(),
        intent_detail: None,
        action_namespace: "podcast.inbox.triage".into(),
        action_body: "{}".into(),
        schedule: "daily".into(),
        next_run_at: None,
        last_run_at: None,
        status: "pending".into(),
        is_enabled: true,
    };
    let json = serde_json::to_string(&task).expect("encode");
    assert!(!json.contains("description"));
    assert!(!json.contains("intent_detail"));
    assert!(!json.contains("action_namespace"));
    assert!(!json.contains("action_body"));
    assert!(!json.contains("next_run_at"));
    assert!(!json.contains("last_run_at"));
    let decoded: AgentTaskSummary = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded.intent_type, "inbox_triage");
    assert_eq!(decoded.intent_label, "Triage inbox");
    assert_eq!(decoded.action_namespace, "");
    assert_eq!(decoded.action_body, "");
}

#[test]
fn knowledge_search_result_round_trips_with_all_fields() {
    let row = KnowledgeSearchResult {
        episode_id: "ep-1".into(),
        episode_title: "Pilot".into(),
        podcast_title: "Some Show".into(),
        snippet: "…the relevant excerpt…".into(),
        start_secs: Some(123.5),
        relevance_score: 0.87,
    };
    let json = serde_json::to_string(&row).expect("encode");
    assert!(json.contains("\"start_secs\":123.5"));
    let decoded: KnowledgeSearchResult = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, row);
}

#[test]
fn knowledge_search_result_omits_none_start_secs() {
    let row = KnowledgeSearchResult {
        episode_id: "ep-1".into(),
        episode_title: "Pilot".into(),
        podcast_title: "Some Show".into(),
        snippet: "x".into(),
        start_secs: None,
        relevance_score: 0.5,
    };
    let json = serde_json::to_string(&row).expect("encode");
    assert!(!json.contains("start_secs"));
    let decoded: KnowledgeSearchResult = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, row);
}

#[test]
fn clip_summary_omits_none_title() {
    let clip = ClipSummary {
        id: "clip-1".into(),
        episode_id: "ep-1".into(),
        episode_title: "Pilot".into(),
        podcast_title: "Some Show".into(),
        start_secs: 10.0,
        end_secs: 70.0,
        title: None,
        transcript_text: String::new(),
        speaker: None,
        source: String::new(),
        refinement_status: String::new(),
        created_at: 1_700_000_000,
    };
    let json = serde_json::to_string(&clip).expect("encode");
    assert!(!json.contains("\"title\""));
    let decoded: ClipSummary = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, clip);
}

#[test]
fn clip_summary_round_trips_with_title() {
    let clip = ClipSummary {
        id: "clip-1".into(),
        episode_id: "ep-1".into(),
        episode_title: "Pilot".into(),
        podcast_title: "Some Show".into(),
        start_secs: 12.5,
        end_secs: 72.5,
        title: Some("Marcus on retrieval".into()),
        transcript_text: "A useful quote.".into(),
        speaker: Some("spk_0".into()),
        source: "auto".into(),
        refinement_status: "transcript_refined".into(),
        created_at: 1_700_000_000,
    };
    let json = serde_json::to_string(&clip).expect("encode");
    assert!(json.contains("\"title\":\"Marcus on retrieval\""));
    assert!(json.contains("\"transcript_text\":\"A useful quote.\""));
    let decoded: ClipSummary = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, clip);
}

#[test]
fn contact_summary_omits_none_optionals() {
    use super::projections::ContactSummary;
    let c = ContactSummary {
        npub: "npub1example".into(),
        pubkey_hex: "aabbccddeeff".into(),
        display_name: None,
        picture_url: None,
    };
    let json = serde_json::to_string(&c).expect("encode");
    assert!(!json.contains("display_name"));
    assert!(!json.contains("picture_url"));
    // pubkey_hex is always serialized (not optional)
    assert!(json.contains("pubkey_hex"));
    assert!(json.contains("aabbccddeeff"));
    let decoded: ContactSummary = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, c);
}

#[test]
fn contact_summary_round_trips_with_metadata() {
    use super::projections::ContactSummary;
    let c = ContactSummary {
        npub: "npub1example".into(),
        pubkey_hex: "deadbeef1234".into(),
        display_name: Some("Satoshi".into()),
        picture_url: Some("https://ex.com/avatar.png".into()),
    };
    let json = serde_json::to_string(&c).expect("encode");
    let decoded: ContactSummary = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, c);
}

#[test]
fn contact_summary_pubkey_hex_matches_npub() {
    use super::projections::ContactSummary;
    use nostr::nips::nip19::ToBech32;

    // Start from a known raw hex pubkey — the same shape `FollowListObserver`
    // reads from `entry.pubkey` (the inner FollowListProjection stores raw hex).
    let hex = "3bf0c63fcb93463407af97a5e5ee64fa883d107ef9e558472c4eb9aaaefa459d";

    // Replicate EXACTLY the observer's npub-encode path (social_handler.rs:177):
    //   nostr::PublicKey::parse(hex) -> to_bech32()
    let pk = nostr::PublicKey::parse(hex).expect("hex parses to a valid pubkey");
    let npub = pk.to_bech32().expect("pubkey encodes to bech32 npub");

    // This is what the observer would emit for this entry.
    let c = ContactSummary {
        npub: npub.clone(),
        pubkey_hex: hex.to_string(),
        display_name: None,
        picture_url: None,
    };

    // ── Key-equivalence assertion (the real guarantee) ───────────────────────
    // Decode the emitted npub back to its 32-byte pubkey, hex-encode it, and
    // assert it equals `pubkey_hex`. This FAILS if pubkey_hex and npub ever
    // describe DIFFERENT keys — not merely a JSON round-trip.
    let decoded_pk = nostr::PublicKey::parse(&c.npub).expect("npub decodes back to a pubkey");
    assert_eq!(
        decoded_pk.to_hex(),
        c.pubkey_hex,
        "npub decoded back to hex must equal pubkey_hex (same key)"
    );
    // And the input hex is exactly what we put in pubkey_hex.
    assert_eq!(c.pubkey_hex, hex, "pubkey_hex must carry the raw entry hex");

    // ── Wire presence assertion (Android claimProfile needs the hex) ─────────
    let json = serde_json::to_string(&c).expect("encode");
    assert!(
        json.contains("\"pubkey_hex\""),
        "pubkey_hex must be serialized (not skipped)"
    );
    assert!(json.contains(hex), "raw hex must appear in the serialized JSON");
    let roundtrip: ContactSummary = serde_json::from_str(&json).expect("decode");
    assert_eq!(roundtrip, c);
}

#[test]
fn social_snapshot_round_trips_with_contacts() {
    use super::projections::ContactSummary;
    let snap = SocialSnapshot {
        following: vec![
            ContactSummary {
                npub: "npub1aaa".into(),
                pubkey_hex: "aaa000".into(),
                display_name: Some("Alice".into()),
                picture_url: None,
            },
            ContactSummary {
                npub: "npub1bbb".into(),
                pubkey_hex: "bbb000".into(),
                display_name: None,
                picture_url: Some("https://ex.com/b.png".into()),
            },
        ],
        following_count: 2,
        approved_pubkeys: Vec::new(),
        blocked_pubkeys: Vec::new(),
    };
    let json = serde_json::to_string(&snap).expect("encode");
    let decoded: SocialSnapshot = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, snap);
}

#[test]
fn social_snapshot_default_is_empty() {
    let snap = SocialSnapshot::default();
    let json = serde_json::to_string(&snap).expect("encode");
    assert!(json.contains("\"following\":[]"));
    assert!(json.contains("\"following_count\":0"));
}

#[test]
fn agent_pick_summary_round_trips_with_all_fields() {
    let pick = AgentPickSummary {
        episode_id: "ep-1".into(),
        episode_title: "Pilot".into(),
        podcast_id: "pod-1".into(),
        podcast_title: "Some Show".into(),
        artwork_url: Some("https://ex.com/art.png".into()),
        published_at: 1_700_000_000,
        duration_secs: Some(3600.0),
        pick_reason: "New from Some Show".into(),
        pick_score: 0.95,
    };
    let json = serde_json::to_string(&pick).expect("encode");
    let decoded: AgentPickSummary = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, pick);
}

#[test]
fn agent_pick_summary_omits_none_optionals() {
    let pick = AgentPickSummary {
        episode_id: "ep-2".into(),
        episode_title: "Untitled".into(),
        podcast_id: "pod-2".into(),
        podcast_title: "No-Art Show".into(),
        artwork_url: None,
        published_at: 1_700_000_000,
        duration_secs: None,
        pick_reason: "New".into(),
        pick_score: 0.5,
    };
    let json = serde_json::to_string(&pick).expect("encode");
    assert!(!json.contains("artwork_url"));
    assert!(!json.contains("duration_secs"));
    let decoded: AgentPickSummary = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, pick);
}

#[test]
fn memory_fact_round_trips() {
    let fact = MemoryFact {
        id: "preferred_genre".into(),
        key: "preferred_genre".into(),
        value: "technology".into(),
        source: "user".into(),
        created_at: 1_700_000_000,
    };
    let json = serde_json::to_string(&fact).expect("encode");
    let decoded: MemoryFact = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, fact);
}

#[test]
fn memory_fact_decodes_agent_source() {
    let json = r#"{"id":"k","key":"k","value":"v","source":"agent","created_at":1700000000}"#;
    let decoded: MemoryFact = serde_json::from_str(json).expect("decode");
    assert_eq!(decoded.source, "agent");
}

#[test]
fn episode_summary_omits_empty_ai_categories() {
    let ep = EpisodeSummary {
        id: "ep-1".into(),
        title: "Pilot".into(),
        ..EpisodeSummary::default()
    };
    let json = serde_json::to_string(&ep).expect("encode");
    assert!(!json.contains("ai_categories"));
}

#[test]
fn episode_summary_round_trips_with_ai_categories() {
    let ep = EpisodeSummary {
        id: "ep-1".into(),
        title: "Pilot".into(),
        ai_categories: vec!["Technology".into(), "Science".into()],
        ..EpisodeSummary::default()
    };
    let json = serde_json::to_string(&ep).expect("encode");
    assert!(json.contains("\"ai_categories\":[\"Technology\",\"Science\"]"));
    let decoded: EpisodeSummary = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, ep);
}

#[test]
fn category_browse_item_round_trips() {
    let item = CategoryBrowseItem {
        category: "Technology".into(),
        episode_count: 12,
        podcast_count: 3,
        top_episode_ids: vec!["ep-1".into(), "ep-2".into(), "ep-3".into()],
        ad_segments: vec![],
    };
    let json = serde_json::to_string(&item).expect("encode");
    assert!(json.contains("\"category\":\"Technology\""));
    assert!(json.contains("\"episode_count\":12"));
    assert!(json.contains("\"podcast_count\":3"));
    let decoded: CategoryBrowseItem = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, item);
}

#[test]
fn episode_summary_played_omitted_when_false() {
    let ep = EpisodeSummary {
        id: "ep-1".into(),
        title: "Ep".into(),
        ..EpisodeSummary::default()
    };
    let json = serde_json::to_string(&ep).expect("encode");
    assert!(
        !json.contains("played"),
        "played=false must be omitted per D5"
    );
}

#[test]
fn episode_summary_played_present_when_true() {
    let ep = EpisodeSummary {
        id: "ep-1".into(),
        title: "Ep".into(),
        played: true,
        ..EpisodeSummary::default()
    };
    let json = serde_json::to_string(&ep).expect("encode");
    assert!(json.contains("\"played\":true"));
    let decoded: EpisodeSummary = serde_json::from_str(&json).expect("decode");
    assert!(decoded.played);
}
