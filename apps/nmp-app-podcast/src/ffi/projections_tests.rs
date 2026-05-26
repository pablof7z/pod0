//! Round-trip + omit-empty tests for [`super::projections`].
//!
//! Kept in a sibling file so `projections.rs` itself stays inside the
//! AGENTS.md 500-line hard limit.

use super::projections::{
    AgentMessageSummary, AgentPickSummary, AgentSnapshot, AgentTaskSummary, CategoryBrowseItem,
    ChapterSummary, ClipSummary, CommentSummary, ContactSummary, EpisodeSummary,
    KnowledgeSearchResult, MemoryFact, NostrShowSummary, SettingsSnapshot, SocialSnapshot,
    TranscriptEntry, TtsEpisodeSummary, WidgetSnapshot, WikiArticle,
};
use crate::player::AdSegment;

#[test]
fn episode_summary_omits_empty_ad_segments() {
    let ep = EpisodeSummary {
        id: "ep-1".into(),
        title: "Pilot".into(),
        ..EpisodeSummary::default()
    };
    let json = serde_json::to_string(&ep).expect("encode");
    assert!(!json.contains("ad_segments"));
}

#[test]
fn episode_summary_round_trips_with_ad_segments() {
    use podcast_core::AdKind;
    let ep = EpisodeSummary {
        id: "ep-1".into(),
        title: "Pilot".into(),
        ad_segments: vec![AdSegment::new(30.0, 60.0, AdKind::Midroll)],
        ..EpisodeSummary::default()
    };
    let json = serde_json::to_string(&ep).expect("encode");
    assert!(json.contains("ad_segments"));
    assert!(json.contains(r#""start_secs":30"#));
    let decoded: EpisodeSummary = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, ep);
}

#[test]
fn widget_snapshot_omits_none_optionals() {
    let widget = WidgetSnapshot {
        now_playing_episode_title: None,
        now_playing_podcast_title: None,
        now_playing_artwork_url: None,
        is_playing: false,
        position_fraction: 0.0,
        unplayed_count: 0,
    };
    let json = serde_json::to_string(&widget).expect("encode");
    assert!(!json.contains("now_playing_episode_title"));
    assert!(!json.contains("now_playing_podcast_title"));
    assert!(!json.contains("now_playing_artwork_url"));
    assert!(json.contains("\"is_playing\":false"));
    assert!(json.contains("\"position_fraction\":0.0"));
    assert!(json.contains("\"unplayed_count\":0"));
}

#[test]
fn episode_summary_omits_none_download_path() {
    let ep = EpisodeSummary {
        id: "ep-1".into(),
        title: "Pilot".into(),
        ..EpisodeSummary::default()
    };
    let json = serde_json::to_string(&ep).expect("encode");
    assert!(!json.contains("download_path"));
}

#[test]
fn episode_summary_round_trips_with_download_path() {
    let ep = EpisodeSummary {
        id: "ep-1".into(),
        title: "Pilot".into(),
        download_path: Some("/var/mobile/Containers/Downloads/ep-1.mp3".into()),
        ..EpisodeSummary::default()
    };
    let json = serde_json::to_string(&ep).expect("encode");
    assert!(json.contains("download_path"));
    let decoded: EpisodeSummary = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, ep);
}

#[test]
fn episode_summary_omits_empty_chapters() {
    let ep = EpisodeSummary {
        id: "ep-1".into(),
        title: "Pilot".into(),
        ..EpisodeSummary::default()
    };
    let json = serde_json::to_string(&ep).expect("encode");
    assert!(!json.contains("chapters"));
}

#[test]
fn episode_summary_omits_none_description() {
    let ep = EpisodeSummary {
        id: "ep-1".into(),
        title: "Pilot".into(),
        ..EpisodeSummary::default()
    };
    let json = serde_json::to_string(&ep).expect("encode");
    assert!(!json.contains("description"));
}

#[test]
fn episode_summary_round_trips_with_chapters() {
    let ep = EpisodeSummary {
        id: "ep-1".into(),
        title: "Pilot".into(),
        chapters: vec![
            ChapterSummary {
                start_secs: 0.0,
                end_secs: Some(60.0),
                title: "Intro".into(),
                image_url: Some("https://ex.com/intro.png".into()),
                url: None,
                is_ai_generated: false,
            },
            ChapterSummary {
                start_secs: 60.0,
                title: "Main".into(),
                ..ChapterSummary::default()
            },
        ],
        ..EpisodeSummary::default()
    };
    let json = serde_json::to_string(&ep).expect("encode");
    let decoded: EpisodeSummary = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, ep);
    assert!(!json.contains("\"url\":null"));
}

#[test]
fn episode_summary_round_trips_with_description() {
    let ep = EpisodeSummary {
        id: "ep-1".into(),
        title: "Pilot".into(),
        description: Some("Welcome to the show.".into()),
        ..EpisodeSummary::default()
    };
    let json = serde_json::to_string(&ep).expect("encode");
    assert!(json.contains("description"));
    let decoded: EpisodeSummary = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, ep);
}

#[test]
fn episode_summary_omits_empty_transcript_fields() {
    let ep = EpisodeSummary {
        id: "ep-1".into(),
        title: "Pilot".into(),
        ..EpisodeSummary::default()
    };
    let json = serde_json::to_string(&ep).expect("encode");
    // No transcript URL and no entries yet — neither field should appear
    // so the wire payload stays byte-compatible with older snapshots.
    assert!(!json.contains("transcript_url"));
    assert!(!json.contains("transcript_entries"));
}

#[test]
fn episode_summary_round_trips_with_transcript_fields() {
    let ep = EpisodeSummary {
        id: "ep-1".into(),
        title: "Pilot".into(),
        transcript_url: Some("https://ex.com/t.vtt".into()),
        transcript_entries: vec![
            TranscriptEntry {
                start_secs: 0.0,
                end_secs: Some(1.5),
                speaker: Some("Host".into()),
                text: "Hello".into(),
            },
            TranscriptEntry {
                start_secs: 1.5,
                end_secs: Some(3.0),
                speaker: None,
                text: "world.".into(),
            },
        ],
        ..EpisodeSummary::default()
    };
    let json = serde_json::to_string(&ep).expect("encode");
    assert!(json.contains("transcript_url"));
    assert!(json.contains("transcript_entries"));
    let decoded: EpisodeSummary = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, ep);
}

#[test]
fn transcript_entry_omits_none_fields() {
    let entry = TranscriptEntry {
        start_secs: 12.0,
        end_secs: None,
        speaker: None,
        text: "hi".into(),
    };
    let json = serde_json::to_string(&entry).expect("encode");
    assert!(!json.contains("end_secs"));
    assert!(!json.contains("speaker"));
    assert!(json.contains("\"start_secs\":12.0"));
    assert!(json.contains("\"text\":\"hi\""));
    let decoded: TranscriptEntry = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, entry);
}

#[test]
fn nostr_show_summary_omits_none_optionals() {
    let row = NostrShowSummary {
        event_id: "ev".into(),
        author_pubkey: "pk".into(),
        title: "Bare".into(),
        ..NostrShowSummary::default()
    };
    let json = serde_json::to_string(&row).expect("encode");
    assert!(!json.contains("description"));
    assert!(!json.contains("feed_url"));
    assert!(!json.contains("artwork_url"));
    assert!(!json.contains("categories"));
    let decoded: NostrShowSummary = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, row);
}

#[test]
fn nostr_show_summary_round_trips_with_all_fields() {
    let row = NostrShowSummary {
        event_id: "ev-1".into(),
        author_pubkey: "pk-1".into(),
        title: "T".into(),
        description: Some("D".into()),
        feed_url: Some("https://x.example/rss".into()),
        artwork_url: Some("https://img.example/c.jpg".into()),
        categories: vec!["Tech".into(), "News".into()],
    };
    let json = serde_json::to_string(&row).expect("encode");
    let decoded: NostrShowSummary = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, row);
}

#[test]
fn nostr_show_summary_decodes_camel_case_wire_for_swift() {
    let row = NostrShowSummary {
        event_id: "ev".into(),
        author_pubkey: "pk".into(),
        title: "T".into(),
        ..Default::default()
    };
    let json = serde_json::to_string(&row).expect("encode");
    assert!(json.contains(r#""event_id":"ev""#));
    assert!(json.contains(r#""author_pubkey":"pk""#));
}

#[test]
fn widget_snapshot_round_trips_with_all_fields() {
    let widget = WidgetSnapshot {
        now_playing_episode_title: Some("Ep 42".into()),
        now_playing_podcast_title: Some("Some Show".into()),
        now_playing_artwork_url: Some("https://ex.com/art.png".into()),
        is_playing: true,
        position_fraction: 0.42,
        unplayed_count: 7,
    };
    let json = serde_json::to_string(&widget).expect("encode");
    let decoded: WidgetSnapshot = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, widget);
}

// ── Agent chat projection (feature #32) ────────────────────────────

#[test]
fn agent_message_summary_round_trips() {
    let msg = AgentMessageSummary {
        id: "msg-1".into(),
        role: "user".into(),
        content: "What's new today?".into(),
        created_at: 1_700_000_000,
        is_generating: false,
    };
    let json = serde_json::to_string(&msg).expect("encode");
    // All fields are always present on the wire — the iOS decoder
    // assumes a stable shape for every message row.
    assert!(json.contains("\"id\":\"msg-1\""));
    assert!(json.contains("\"role\":\"user\""));
    assert!(json.contains("\"content\":\"What's new today?\""));
    assert!(json.contains("\"created_at\":1700000000"));
    assert!(json.contains("\"is_generating\":false"));
    let decoded: AgentMessageSummary = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, msg);
}

#[test]
fn agent_snapshot_round_trips_with_messages() {
    let snap = AgentSnapshot {
        messages: vec![
            AgentMessageSummary {
                id: "m1".into(),
                role: "user".into(),
                content: "hi".into(),
                created_at: 1,
                is_generating: false,
            },
            AgentMessageSummary {
                id: "m2".into(),
                role: "assistant".into(),
                content: "I'm thinking…".into(),
                created_at: 2,
                is_generating: true,
            },
        ],
        is_busy: true,
    };
    let json = serde_json::to_string(&snap).expect("encode");
    let decoded: AgentSnapshot = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, snap);
}

#[test]
fn agent_snapshot_default_has_empty_transcript() {
    let snap = AgentSnapshot::default();
    assert!(snap.messages.is_empty());
    assert!(!snap.is_busy);
    let json = serde_json::to_string(&snap).expect("encode");
    // Even when empty the shape stays stable — `messages` must be `[]`
    // (not absent) and `is_busy` must be `false` on the wire so the
    // Swift decoder doesn't have to handle a missing key.
    assert!(json.contains("\"messages\":[]"));
    assert!(json.contains("\"is_busy\":false"));
}

#[test]
fn episode_summary_omits_none_playback_position() {
    let ep = EpisodeSummary {
        id: "ep-1".into(),
        title: "Pilot".into(),
        ..EpisodeSummary::default()
    };
    let json = serde_json::to_string(&ep).expect("encode");
    assert!(!json.contains("playback_position_secs"));
}

#[test]
fn episode_summary_round_trips_with_playback_position() {
    let ep = EpisodeSummary {
        id: "ep-1".into(),
        title: "Pilot".into(),
        playback_position_secs: Some(123.5),
        ..EpisodeSummary::default()
    };
    let json = serde_json::to_string(&ep).expect("encode");
    assert!(json.contains("\"playback_position_secs\":123.5"));
    let decoded: EpisodeSummary = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, ep);
}

#[test]
fn settings_snapshot_round_trips() {
    let s = SettingsSnapshot { has_completed_onboarding: true, ..SettingsSnapshot::default() };
    let json = serde_json::to_string(&s).expect("encode");
    assert!(json.contains("\"has_completed_onboarding\":true"));
    let decoded: SettingsSnapshot = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, s);
}

#[test]
fn settings_snapshot_default_is_fresh_install() {
    let s = SettingsSnapshot::default();
    assert!(!s.has_completed_onboarding);
    let json = serde_json::to_string(&s).expect("encode");
    assert!(json.contains("\"has_completed_onboarding\":false"));
}

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
