//! Round-trip + omit-empty tests for [`super::projections`] (part 1/2).
//!
//! Kept in a sibling file so `projections.rs` itself stays inside the
//! AGENTS.md 500-line hard limit. Tests for remaining types live in
//! `projections_tests_ext.rs`.

use super::projections::{
    AgentMessageSummary, AgentSnapshot, ChapterSummary, EpisodeSummary,
    NostrShowSummary, SettingsSnapshot, TranscriptEntry, WidgetSnapshot,
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
                ..ChapterSummary::default()
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
