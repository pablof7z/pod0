//! Tests for [`super::snapshot`] — `PodcastUpdate` round-trip + per-field
//! byte-identity coverage (part 1/2).
//!
//! Split out of `snapshot.rs` to keep that file under the 500-line hard
//! limit. Comments, queue, picks, memory, clips, and inbox
//! tests live in `snapshot_tests_ext.rs`.

use super::PodcastUpdate;
use crate::ffi::projections::{
    AgentMessageSummary, AgentSnapshot, AgentTaskSummary, ConversationsSnapshot,
    DownloadItemSnapshot, DownloadQueueSnapshot, EpisodeSummary, PendingApprovalSnapshot,
    SettingsSnapshot, VoiceState, WidgetSnapshot,
};
use crate::ffi::snapshot::provider_key_present;
use crate::player::PlayerState;

#[test]
fn episode_summary_field_round_trips() {
    // Guards the `summary` projection field end-to-end on the wire: present
    // when set (key emitted), omitted when None (D5 skip_serializing_if), and
    // decoded back. Catches a dropped/renamed serde attr on the field that the
    // compiler-checked struct literal in `build_snapshot_payload` would not.
    let ep = EpisodeSummary {
        id: "ep-1".into(),
        title: "T".into(),
        summary: Some("A concise summary.".into()),
        ..Default::default()
    };
    let json = serde_json::to_string(&ep).expect("encode");
    assert!(json.contains("\"summary\":\"A concise summary.\""));
    let decoded: EpisodeSummary = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded.summary.as_deref(), Some("A concise summary."));

    // None ⇒ omitted from the wire (byte-compat with pre-summary snapshots).
    let bare = EpisodeSummary {
        id: "ep-2".into(),
        ..Default::default()
    };
    let json = serde_json::to_string(&bare).expect("encode");
    assert!(!json.contains("summary"));
}

#[test]
fn default_snapshot_omits_now_playing() {
    let value: serde_json::Value =
        serde_json::from_str(&serde_json::to_string(&PodcastUpdate::default()).expect("encode"))
            .expect("valid json");
    let obj = value.as_object().expect("object");
    // `skip_serializing_if = "Option::is_none"` keeps optional fields out of
    // the empty payload.
    assert_eq!(obj["running"], serde_json::json!(true));
    assert_eq!(obj["rev"], serde_json::json!(0));
    assert_eq!(obj["schema_version"], serde_json::json!(1));
    assert!(!obj.contains_key("now_playing"));
    assert!(!obj.contains_key("downloads"));
    // Behavior change: `settings` is the lone non-Option projection and is now
    // always present (it carries `#[serde(default)]` but no skip guard).
    assert!(obj.contains_key("settings"));
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
fn configured_relays_omitted_when_empty() {
    // D5/D6 byte-identity: an empty relay list is absent from the wire so the
    // no-op snapshot stays compatible with the legacy stub and the Swift
    // `decodeIfPresent ?? []` default.
    let json = serde_json::to_string(&PodcastUpdate::default()).expect("encode");
    assert!(!json.contains("configured_relays"));
}

#[test]
fn configured_relays_round_trips_with_role() {
    use super::AppRelayRow;
    let snap = PodcastUpdate {
        configured_relays: vec![
            AppRelayRow {
                url: "wss://relay.primal.net".into(),
                role: "both,indexer".into(),
            },
            AppRelayRow {
                url: "wss://purplepag.es".into(),
                role: "indexer".into(),
            },
        ],
        ..PodcastUpdate::default()
    };
    let json = serde_json::to_string(&snap).expect("encode");
    assert!(json.contains(r#""configured_relays""#));
    assert!(json.contains(r#""role":"both,indexer""#));
    let decoded: PodcastUpdate = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded.configured_relays, snap.configured_relays);
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
    assert!(decoded.widget.is_none());
    assert!(decoded.toast.is_none());
    // `settings` is non-Option but defaults to fresh-install state when
    // omitted from the wire, so older binaries see "not onboarded yet".
    assert_eq!(decoded.settings, SettingsSnapshot::default());
}

#[test]
fn snapshot_with_settings_round_trips() {
    let snap = PodcastUpdate {
        settings: SettingsSnapshot {
            has_completed_onboarding: true,
            ..SettingsSnapshot::default()
        },
        ..PodcastUpdate::default()
    };
    let json = serde_json::to_string(&snap).expect("encode");
    assert!(json.contains("\"settings\""));
    assert!(json.contains("\"has_completed_onboarding\":true"));
    let decoded: PodcastUpdate = serde_json::from_str(&json).expect("decode");
    assert!(decoded.settings.has_completed_onboarding);
}

#[test]
fn snapshot_always_includes_settings() {
    // Behavior change: the `settings` projection is no longer omitted when it
    // equals the fresh-install default — it is always present on the wire so
    // the shell never has to distinguish "absent" from "default".
    let json = serde_json::to_string(&PodcastUpdate::default()).expect("encode");
    assert!(json.contains("\"settings\""));
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
        title: "Inbox Triage".into(),
        description: Some("Surface new episodes worth your time".into()),
        intent_type: "inbox_triage".into(),
        intent_label: "Triage inbox".into(),
        intent_detail: Some("Prioritize new episodes".into()),
        action_namespace: "podcast.inbox.triage".into(),
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
    assert!(json.contains("\"intent_type\":\"inbox_triage\""));
    assert!(!json.contains("action_namespace"));
    assert!(!json.contains("action_body"));
    let decoded: PodcastUpdate = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded.agent_tasks.len(), 1);
    assert_eq!(decoded.agent_tasks[0].intent_type, "inbox_triage");
    assert_eq!(decoded.agent_tasks[0].action_namespace, "");
    assert_eq!(decoded.agent_tasks[0].action_body, "");
    assert!(decoded.memory_facts.is_empty());
}

#[test]
fn snapshot_with_auto_skip_ads_round_trips() {
    let snap = PodcastUpdate {
        settings: SettingsSnapshot {
            auto_skip_ads_enabled: true,
            ..SettingsSnapshot::default()
        },
        ..PodcastUpdate::default()
    };
    let json = serde_json::to_string(&snap).expect("encode");
    assert!(json.contains("\"auto_skip_ads_enabled\":true"));
    let decoded: PodcastUpdate = serde_json::from_str(&json).expect("decode");
    assert!(decoded.settings.auto_skip_ads_enabled);
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
        now_playing_chapter_title: Some("Chapter 2".into()),
        is_playing: true,
        position_fraction: 0.42,
        position_secs: 504.0,
        duration_secs: 1200.0,
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
            kind: Default::default(),
            url: "https://example.com/ep-1.mp3".into(),
            progress: 0.5,
            state: "active".into(),
            total_bytes: None,
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
        kind: Default::default(),
        url: String::new(),
        progress: 0.0,
        state: "queued".into(),
        total_bytes: None,
        error: None,
    };
    let json = serde_json::to_string(&item).expect("encode");
    assert!(!json.contains("error"));
    // Empty `url` is skipped on the wire (pull-model field; iOS never sets it).
    assert!(!json.contains("url"));
    let decoded: DownloadItemSnapshot = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, item);
}

// ── Voice snapshot wiring (M8.A) ───────────────

#[test]
fn snapshot_with_voice_round_trips() {
    let voice = VoiceState {
        is_speaking: true,
        current_request_id: Some("req-1".into()),
        current_voice_id: Some("rachel".into()),
        ..VoiceState::default()
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
        ..VoiceState::default()
    };
    let json = serde_json::to_string(&v).expect("encode");
    assert!(!json.contains("current_request_id"));
    assert!(!json.contains("current_voice_id"));
    assert!(json.contains("\"is_speaking\":false"));
    let decoded: VoiceState = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, v);
}

#[test]
fn settings_snapshot_defaults_are_30_and_15() {
    let s = SettingsSnapshot::default();
    assert!((s.skip_forward_secs - 30.0).abs() < f64::EPSILON);
    assert!((s.skip_backward_secs - 15.0).abs() < f64::EPSILON);
}

#[test]
fn settings_snapshot_skip_intervals_round_trip() {
    let snap = PodcastUpdate {
        settings: SettingsSnapshot {
            skip_forward_secs: 45.0,
            skip_backward_secs: 10.0,
            ..SettingsSnapshot::default()
        },
        ..PodcastUpdate::default()
    };
    let json = serde_json::to_string(&snap).expect("encode");
    assert!(
        json.contains("\"skip_forward_secs\":45.0") || json.contains("\"skip_forward_secs\":45")
    );
    let decoded: PodcastUpdate = serde_json::from_str(&json).expect("decode");
    assert!((decoded.settings.skip_forward_secs - 45.0).abs() < f64::EPSILON);
    assert!((decoded.settings.skip_backward_secs - 10.0).abs() < f64::EPSILON);
}

#[test]
fn settings_snapshot_missing_skip_fields_use_defaults() {
    // Simulate an old on-disk JSON without skip fields
    let json = r#"{"has_completed_onboarding":false,"auto_skip_ads_enabled":false}"#;
    let s: SettingsSnapshot = serde_json::from_str(json).expect("decode");
    assert!((s.skip_forward_secs - 30.0).abs() < f64::EPSILON);
    assert!((s.skip_backward_secs - 15.0).abs() < f64::EPSILON);
}

#[test]
fn provider_key_presence_is_trimmed() {
    assert!(!provider_key_present(None));
    assert!(!provider_key_present(Some("")));
    assert!(!provider_key_present(Some("   ")));
    assert!(provider_key_present(Some(" sk-live ")));
}
