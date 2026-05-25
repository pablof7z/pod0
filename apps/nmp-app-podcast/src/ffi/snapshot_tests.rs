//! Tests for [`super::snapshot`] — `PodcastUpdate` round-trip + per-field
//! byte-identity coverage.
//!
//! Split out of `snapshot.rs` to keep that file under the 500-line hard
//! limit as new projections (queue, …) accrete onto the typed root.

use super::projections::{
    BriefingSegmentSummary, BriefingSnapshot, CommentSummary, ConversationsSnapshot,
    DownloadItemSnapshot, DownloadQueueSnapshot, PendingApprovalSnapshot, SettingsSnapshot,
    VoiceState, WidgetSnapshot,
    BriefingSnapshot, ConversationsSnapshot, DownloadItemSnapshot, DownloadQueueSnapshot,
    PendingApprovalSnapshot, VoiceState, WidgetSnapshot, WikiArticle,
};
use super::snapshot::PodcastUpdate;
use crate::player::PlayerState;
//! Tests for `super::snapshot::PodcastUpdate` round-trips. Lives in a sibling
//! file so [`super::snapshot`] stays under the 500-line hard cap.

use super::*;
use super::super::projections::{
    AgentPickSummary, DownloadItemSnapshot, PendingApprovalSnapshot,
};
//! Tests for [`super::snapshot`]. Lives in its own file so
//! `snapshot.rs` stays under the 500-line hard limit as new
//! projections land.
//!
//! Included from `snapshot.rs` via `#[cfg(test)] #[path = "snapshot_tests.rs"]
//! mod tests;` — there is no `mod snapshot_tests;` in `ffi/mod.rs` and
//! none is wanted. The `#[path]` attribute makes this file act as the
//! `tests` submodule of `snapshot`, so it inherits `super::*`.

use super::snapshot::*;
use super::projections::{
    AgentTaskSummary, BriefingSnapshot, ConversationsSnapshot, DownloadItemSnapshot,
    DownloadQueueSnapshot, PendingApprovalSnapshot, VoiceState, WidgetSnapshot,
};
use crate::player::PlayerState;
//! Snapshot round-trip + byte-identity tests for [`super::snapshot`].
//!
//! Lifted out of `snapshot.rs` so the production module stays under the
//! 500-LOC hard cap. Wired in via `#[cfg(test)] mod snapshot_tests;` from
//! `super::mod`.

use super::projections::{
    BriefingSnapshot, ConversationsSnapshot, DownloadItemSnapshot, DownloadQueueSnapshot,
    MemoryFact, PendingApprovalSnapshot, VoiceState, WidgetSnapshot,
};
//! Snapshot tests — pulled into a sibling file with `#[path]` to keep
//! `snapshot.rs` under the 500-LOC hard ceiling. The test module
//! semantics are unchanged (it's still `mod tests` inside `snapshot.rs`).

use super::projections::{
    BriefingSnapshot, ClipSummary, ConversationsSnapshot, DownloadItemSnapshot,
    DownloadQueueSnapshot, PendingApprovalSnapshot, VoiceState, WidgetSnapshot,
    InboxItem, PendingApprovalSnapshot, VoiceState, WidgetSnapshot,
    AgentMessageSummary, AgentSnapshot, BriefingSnapshot, ConversationsSnapshot,
    DownloadItemSnapshot, DownloadQueueSnapshot, PendingApprovalSnapshot, VoiceState,
    WidgetSnapshot,
    PendingApprovalSnapshot, SettingsSnapshot, VoiceState, WidgetSnapshot,
};
use super::snapshot::PodcastUpdate;
use crate::player::PlayerState;

#[test]
fn default_snapshot_omits_now_playing() {
    let json = serde_json::to_string(&PodcastUpdate::default()).expect("encode");
    // `skip_serializing_if = "Option::is_none"` keeps the empty
    // payload byte-identical to the legacy stub.
    // `skip_serializing_if = "Option::is_none"` keeps the empty payload
    // byte-identical to the legacy stub.
    assert_eq!(json, r#"{"running":true,"rev":0,"schema_version":1}"#);
}

#[test]
fn snapshot_with_now_playing_round_trips() {
    let mut state = PlayerState::idle();
    state.episode_id = Some("ep-1".into());
    state.url = Some("https://ex.com/ep-1.mp3".into());
    state.position_secs = 12.0;
    state.is_playing = true;

    let snap = PodcastUpdate {
        now_playing: Some(state.clone()),
        ..PodcastUpdate::default()
    };
    let json = serde_json::to_string(&snap).expect("encode");
    let decoded: PodcastUpdate = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded.now_playing, Some(state));
    assert!(decoded.running);
    assert_eq!(decoded.schema_version, 1);
}

#[test]
fn default_update_serializes_to_valid_json() {
    let payload = serde_json::to_string(&PodcastUpdate::default()).expect("encode");
    let _decoded: PodcastUpdate = serde_json::from_str(&payload).expect("decode");
}

#[test]
fn snapshot_decoder_tolerates_unknown_fields() {
    // Forward-compat: an older binary decoding a newer snapshot ignores
    // fields it doesn't know about (Codable parity).
    let payload = r#"{"running":true,"rev":7,"schema_version":1,"future_field":"ignored"}"#;
    let decoded: PodcastUpdate = serde_json::from_str(payload).expect("decode");
    assert_eq!(decoded.rev, 7);
    assert!(decoded.now_playing.is_none());
    assert!(decoded.downloads.is_none());
    assert!(decoded.agent.is_none());
    assert!(decoded.voice.is_none());
    assert!(decoded.briefing.is_none());
    assert!(decoded.widget.is_none());
    assert!(decoded.toast.is_none());
    // `settings` is non-Option but defaults to fresh-install state when
    // omitted from the wire, so older binaries see "not onboarded yet".
    assert_eq!(decoded.settings, SettingsSnapshot::default());
}

#[test]
fn snapshot_with_settings_round_trips() {
    let snap = PodcastUpdate {
        settings: SettingsSnapshot { has_completed_onboarding: true },
        ..PodcastUpdate::default()
    };
    let json = serde_json::to_string(&snap).expect("encode");
    assert!(json.contains("\"settings\""));
    assert!(json.contains("\"has_completed_onboarding\":true"));
    let decoded: PodcastUpdate = serde_json::from_str(&json).expect("decode");
    assert!(decoded.settings.has_completed_onboarding);
}

#[test]
fn snapshot_omits_default_settings() {
    // D6 byte-identity: a fresh-install snapshot must not emit the
    // `settings` key at all so the empty payload stays identical to
    // the legacy stub.
    let json = serde_json::to_string(&PodcastUpdate::default()).expect("encode");
    assert!(!json.contains("settings"));
    assert!(decoded.comments.is_empty());
    assert!(decoded.agent_tasks.is_empty());
}

#[test]
fn default_snapshot_omits_agent_tasks() {
    let json = serde_json::to_string(&PodcastUpdate::default()).expect("encode");
    assert!(!json.contains("agent_tasks"));
}

#[test]
fn snapshot_with_agent_tasks_round_trips() {
    let tasks = vec![AgentTaskSummary {
        id: "task-1".into(),
        title: "Morning Briefing".into(),
        description: Some("Daily digest".into()),
        action_namespace: "podcast.briefings.generate".into(),
        action_body: "{}".into(),
        schedule: "daily".into(),
        next_run_at: Some(1_700_000_000),
        last_run_at: None,
        status: "pending".into(),
        is_enabled: true,
    }];
    let snap = PodcastUpdate {
        agent_tasks: tasks.clone(),
        ..PodcastUpdate::default()
    };
    let json = serde_json::to_string(&snap).expect("encode");
    assert!(json.contains("\"agent_tasks\":["));
    let decoded: PodcastUpdate = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded.agent_tasks, tasks);
    assert!(decoded.memory_facts.is_empty());
}

#[test]
fn snapshot_with_settings_round_trips() {
    let settings = SettingsSnapshot {
        auto_skip_ads_enabled: true,
    };
    let snap = PodcastUpdate {
        settings: Some(settings.clone()),
        ..PodcastUpdate::default()
    };
    let json = serde_json::to_string(&snap).expect("encode");
    assert!(json.contains("\"auto_skip_ads_enabled\":true"));
    let decoded: PodcastUpdate = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded.settings, Some(settings));
}

#[test]
fn default_snapshot_omits_settings() {
    // D5 byte-identity: a snapshot with no settings projection must
    // not bloat the wire payload with `"settings":null`.
    let json = serde_json::to_string(&PodcastUpdate::default()).expect("encode");
    assert!(!json.contains("settings"));
}

#[test]
fn snapshot_with_toast_round_trips() {
    let snap = PodcastUpdate {
        toast: Some("Nothing to resume".into()),
        ..PodcastUpdate::default()
    };
    let json = serde_json::to_string(&snap).expect("encode");
    assert!(json.contains("\"toast\":\"Nothing to resume\""));
    let decoded: PodcastUpdate = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded.toast, Some("Nothing to resume".to_owned()));
}

#[test]
fn snapshot_omits_none_toast() {
    // D5 byte-identity: empty toast must not bloat the wire payload.
    let json = serde_json::to_string(&PodcastUpdate::default()).expect("encode");
    assert!(!json.contains("toast"));
}

#[test]
fn snapshot_with_widget_round_trips() {
    let widget = WidgetSnapshot {
        now_playing_episode_title: Some("Ep 42".into()),
        now_playing_podcast_title: Some("Some Show".into()),
        now_playing_artwork_url: Some("https://ex.com/art.png".into()),
        is_playing: true,
        position_fraction: 0.42,
        unplayed_count: 7,
    };
    let snap = PodcastUpdate {
        widget: Some(widget.clone()),
        ..PodcastUpdate::default()
    };
    let json = serde_json::to_string(&snap).expect("encode");
    let decoded: PodcastUpdate = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded.widget, Some(widget));
}

#[test]
fn snapshot_with_agent_round_trips() {
    let agent = AgentSnapshot {
        messages: vec![
            AgentMessageSummary {
                id: "m1".into(),
                role: "user".into(),
                content: "hello".into(),
                created_at: 1_700_000_000,
                is_generating: false,
            },
            AgentMessageSummary {
                id: "m2".into(),
                role: "assistant".into(),
                content: "I'm thinking about your question…".into(),
                created_at: 1_700_000_001,
                is_generating: false,
            },
        ],
        is_busy: false,
    };
    let snap = PodcastUpdate {
        agent: Some(agent.clone()),
        ..PodcastUpdate::default()
    };
    let json = serde_json::to_string(&snap).expect("encode");
    let decoded: PodcastUpdate = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded.agent, Some(agent));
}

#[test]
fn conversations_snapshot_legacy_shape_still_round_trips() {
    // `ConversationsSnapshot` is the legacy multi-conversation projection
    // shape kept available for the future `ConversationActor`-backed
    // surface (it is no longer wired into `PodcastUpdate.agent`).
    // Lock its public Codable contract so the type isn't accidentally
    // broken while the new `AgentSnapshot` projection lives in parallel.
    let convo = ConversationsSnapshot {
        active_count: 0,
        pending_approvals: vec![],
        latest_conversation_id: None,
    };
    let json = serde_json::to_string(&convo).expect("encode");
    assert!(!json.contains("latest_conversation_id"));
    assert!(json.contains("\"active_count\":0"));
    assert!(json.contains("\"pending_approvals\":[]"));
    let _: ConversationsSnapshot = serde_json::from_str(&json).expect("decode");
    let _ = PendingApprovalSnapshot::default();
}

#[test]
fn snapshot_with_downloads_round_trips() {
    let downloads = DownloadQueueSnapshot {
        active: vec![DownloadItemSnapshot {
            episode_id: "ep-1".into(),
            progress: 0.5,
            state: "active".into(),
            error: None,
        }],
        queued_count: 2,
        completed_today: 0,
    };
    let snap = PodcastUpdate {
        downloads: Some(downloads.clone()),
        ..PodcastUpdate::default()
    };
    let json = serde_json::to_string(&snap).expect("encode");
    let decoded: PodcastUpdate = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded.downloads, Some(downloads));
}

#[test]
fn download_item_snapshot_omits_none_error() {
    let item = DownloadItemSnapshot {
        episode_id: "ep-1".into(),
        progress: 0.0,
        state: "queued".into(),
        error: None,
    };
    let json = serde_json::to_string(&item).expect("encode");
    assert!(!json.contains("error"));
    let decoded: DownloadItemSnapshot = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, item);
}

// ── Voice / briefing snapshot wiring (M8.A + M9.A) ───────────────

#[test]
fn snapshot_with_voice_round_trips() {
    let voice = VoiceState {
        is_speaking: true,
        current_request_id: Some("req-1".into()),
        current_voice_id: Some("rachel".into()),
    };
    let snap = PodcastUpdate {
        voice: Some(voice.clone()),
        ..PodcastUpdate::default()
    };
    let json = serde_json::to_string(&snap).expect("encode");
    let decoded: PodcastUpdate = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded.voice, Some(voice));
}

#[test]
fn voice_state_omits_none_fields() {
    let v = VoiceState {
        is_speaking: false,
        current_request_id: None,
        current_voice_id: None,
    };
    let json = serde_json::to_string(&v).expect("encode");
    assert!(!json.contains("current_request_id"));
    assert!(!json.contains("current_voice_id"));
    assert!(json.contains("\"is_speaking\":false"));
    let decoded: VoiceState = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, v);
}

#[test]
fn snapshot_with_briefing_round_trips() {
    let b = BriefingSnapshot {
        status: "generating".into(),
        is_generating: true,
        segment_count: 0,
        segments: vec![],
        last_generated_at: None,
        segment_count: 0,
        next_scheduled_minutes: Some(45),
    };
    let snap = PodcastUpdate {
        briefing: Some(b.clone()),
        ..PodcastUpdate::default()
    };
    let json = serde_json::to_string(&snap).expect("encode");
    let decoded: PodcastUpdate = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded.briefing, Some(b));
}

#[test]
fn briefing_snapshot_omits_none_next_scheduled() {
    let b = BriefingSnapshot {
        status: "pending".into(),
        is_generating: false,
        segment_count: 0,
        segments: vec![],
        last_generated_at: None,
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
    // D5 byte-identity: empty comments must not bloat the wire payload.
    let json = serde_json::to_string(&PodcastUpdate::default()).expect("encode");
    assert!(!json.contains("\"comments\""));
}

#[test]
fn briefing_snapshot_omits_none_next_scheduled() {
    let b = BriefingSnapshot {
        status: "pending".into(),
        segment_count: 0,
        next_scheduled_minutes: None,
    };
    let json = serde_json::to_string(&b).expect("encode");
    assert!(!json.contains("next_scheduled_minutes"));
    assert!(!json.contains("last_generated_at"));
    assert!(!json.contains("\"segments\""));
    let decoded: BriefingSnapshot = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, b);
}

#[test]
fn briefing_snapshot_with_segments_round_trips() {
    let b = BriefingSnapshot {
        status: "ready".into(),
        is_generating: false,
        segment_count: 2,
        segments: vec![
            BriefingSegmentSummary {
                kind: "intro".into(),
                text: "Good morning.".into(),
                podcast_title: None,
                episode_title: None,
            },
            BriefingSegmentSummary {
                kind: "episode_summary".into(),
                text: "Today on Hard Fork…".into(),
                podcast_title: Some("Hard Fork".into()),
                episode_title: Some("Ep 42".into()),
            },
        ],
        last_generated_at: Some(1_700_000_000),
        next_scheduled_minutes: None,
    };
    let json = serde_json::to_string(&b).expect("encode");
    assert!(json.contains("\"kind\":\"intro\""));
    assert!(json.contains("\"podcast_title\":\"Hard Fork\""));
    assert!(json.contains("\"last_generated_at\":1700000000"));
    let decoded: BriefingSnapshot = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, b);
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
    let snap = PodcastUpdate {
        queue: vec!["ep-1".into(), "ep-2".into(), "ep-3".into()],
        ..PodcastUpdate::default()
    };
    let json = serde_json::to_string(&snap).expect("encode");
    assert!(json.contains(r#""queue":["ep-1","ep-2","ep-3"]"#));
    let decoded: PodcastUpdate = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded.queue, snap.queue);
}
    let decoded: BriefingSnapshot = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, b);

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
        }],
        ..PodcastUpdate::default()
    };
    let json = serde_json::to_string(&snap).expect("encode");
    assert!(json.contains(r#""wiki_search_results""#));
    let decoded: PodcastUpdate = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded.wiki_search_results, snap.wiki_search_results);
// ── AgentPickSummary snapshot wiring (feature #46) ───────────────
//
// Picks-field-on-PodcastUpdate round-tripping is covered together with
// the default-omit byte-compat guarantee.

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
    let decoded: BriefingSnapshot = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, b);
// ── Agent memory (feature #33) ───────────────────────────────────

#[test]
fn snapshot_omits_empty_memory_facts() {
    // D5 byte-identity: empty memory bag must not pollute the wire.
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
    // D5 byte-identity: empty clips list must not show up on the wire.
    let json = serde_json::to_string(&PodcastUpdate::default()).expect("encode");
    assert!(!json.contains("clips"));

#[test]
fn snapshot_with_inbox_round_trips_and_empty_is_omitted() {
    // Empty inbox stays off the wire (D5 byte-identity).
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
