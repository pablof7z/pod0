//! Tests for [`super::snapshot`] — `PodcastUpdate` round-trip + per-field
//! byte-identity coverage.
//!
//! Split out of `snapshot.rs` to keep that file under the 500-line hard
//! limit as new projections (queue, …) accrete onto the typed root.

use super::projections::{
    BriefingSegmentSummary, BriefingSnapshot, ConversationsSnapshot, DownloadItemSnapshot,
    DownloadQueueSnapshot, PendingApprovalSnapshot, SettingsSnapshot, VoiceState, WidgetSnapshot,
};
use super::snapshot::PodcastUpdate;
use crate::player::PlayerState;

#[test]
fn default_snapshot_omits_now_playing() {
    let json = serde_json::to_string(&PodcastUpdate::default()).expect("encode");
    // `skip_serializing_if = "Option::is_none"` keeps the empty
    // payload byte-identical to the legacy stub.
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
    let agent = ConversationsSnapshot {
        active_count: 2,
        pending_approvals: vec![PendingApprovalSnapshot {
            id: "ap-1".into(),
            description: "publish".into(),
            requested_at: 1_700_000_000,
        }],
        latest_conversation_id: Some("conv-1".into()),
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
fn pending_approval_snapshot_omits_unset_fields() {
    let agent = ConversationsSnapshot {
        active_count: 0,
        pending_approvals: vec![],
        latest_conversation_id: None,
    };
    let json = serde_json::to_string(&agent).expect("encode");
    // `latest_conversation_id: None` should be skipped; the other
    // fields are always present.
    assert!(!json.contains("latest_conversation_id"));
    assert!(json.contains("\"active_count\":0"));
    assert!(json.contains("\"pending_approvals\":[]"));
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
