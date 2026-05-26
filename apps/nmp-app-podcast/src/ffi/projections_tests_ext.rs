//! Continuation of round-trip tests for [`super::projections`] (part 2/2).
//!
//! Split from `projections_tests.rs` to keep both files under the 500-line
//! AGENTS.md hard limit.

use super::projections::{
    AgentPickSummary, AgentTaskSummary, CategoryBrowseItem, ChapterSummary, ClipSummary,
    CommentSummary, EpisodeSummary, KnowledgeSearchResult, MemoryFact,
    SocialSnapshot, TtsEpisodeSummary, WikiArticle,
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
fn agent_task_summary_round_trips_with_all_fields() {
    let task = AgentTaskSummary {
        id: "task-1".into(),
        title: "Morning Briefing".into(),
        description: Some("Generate a briefing every morning".into()),
        action_namespace: "podcast.briefings.generate".into(),
        action_body: "{}".into(),
        schedule: "daily".into(),
        next_run_at: Some(1_700_000_000),
        last_run_at: Some(1_699_900_000),
        status: "completed".into(),
        is_enabled: true,
    };
    let json = serde_json::to_string(&task).expect("encode");
    let decoded: AgentTaskSummary = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, task);
}

#[test]
fn agent_task_summary_omits_none_optionals() {
    let task = AgentTaskSummary {
        id: "task-1".into(),
        title: "Inbox Triage".into(),
        description: None,
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
    assert!(!json.contains("next_run_at"));
    assert!(!json.contains("last_run_at"));
    let decoded: AgentTaskSummary = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, task);
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
        created_at: 1_700_000_000,
    };
    let json = serde_json::to_string(&clip).expect("encode");
    assert!(json.contains("\"title\":\"Marcus on retrieval\""));
    let decoded: ClipSummary = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, clip);
}

#[test]
fn contact_summary_omits_none_optionals() {
    use super::projections::ContactSummary;
    let c = ContactSummary {
        npub: "npub1example".into(),
        display_name: None,
        picture_url: None,
    };
    let json = serde_json::to_string(&c).expect("encode");
    assert!(!json.contains("display_name"));
    assert!(!json.contains("picture_url"));
    let decoded: ContactSummary = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, c);
}

#[test]
fn contact_summary_round_trips_with_metadata() {
    use super::projections::ContactSummary;
    let c = ContactSummary {
        npub: "npub1example".into(),
        display_name: Some("Satoshi".into()),
        picture_url: Some("https://ex.com/avatar.png".into()),
    };
    let json = serde_json::to_string(&c).expect("encode");
    let decoded: ContactSummary = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, c);
}

#[test]
fn social_snapshot_round_trips_with_contacts() {
    use super::projections::ContactSummary;
    let snap = SocialSnapshot {
        following: vec![
            ContactSummary {
                npub: "npub1aaa".into(),
                display_name: Some("Alice".into()),
                picture_url: None,
            },
            ContactSummary {
                npub: "npub1bbb".into(),
                display_name: None,
                picture_url: Some("https://ex.com/b.png".into()),
            },
        ],
        following_count: 2,
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
fn wiki_article_omits_empty_sources_on_wire() {
    let article = WikiArticle {
        id: "art-1".into(),
        podcast_id: "pod-1".into(),
        topic: "Bitcoin halvings".into(),
        summary: "Stub summary.".into(),
        source_episode_ids: vec![],
        last_updated_at: 1_700_000_000,
        is_generating: false,
    };
    let json = serde_json::to_string(&article).expect("encode");
    assert!(!json.contains("source_episode_ids"));
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
fn tts_episode_summary_round_trips_with_all_fields() {
    let ep = TtsEpisodeSummary {
        id: "tts-1".into(),
        title: "AI Roundup".into(),
        script: "Hello, this is your daily roundup.".into(),
        duration_estimate_secs: 300.0,
        created_at: 1_700_000_000,
        status: "ready".into(),
        voice_id: Some("rachel".into()),
    };
    let json = serde_json::to_string(&ep).expect("encode");
    let decoded: TtsEpisodeSummary = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, ep);
}

#[test]
fn tts_episode_summary_omits_none_voice_id() {
    let ep = TtsEpisodeSummary {
        id: "tts-1".into(),
        title: "Generated".into(),
        script: "hi".into(),
        duration_estimate_secs: 60.0,
        created_at: 0,
        status: "ready".into(),
        voice_id: None,
    };
    let json = serde_json::to_string(&ep).expect("encode");
    assert!(!json.contains("voice_id"));
    let decoded: TtsEpisodeSummary = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, ep);
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
    let ep = EpisodeSummary { id: "ep-1".into(), title: "Ep".into(), ..EpisodeSummary::default() };
    let json = serde_json::to_string(&ep).expect("encode");
    assert!(!json.contains("played"), "played=false must be omitted per D5");
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
