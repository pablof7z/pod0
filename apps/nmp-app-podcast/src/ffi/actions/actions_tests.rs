//! Wire-contract tests for [`super`] action payloads and action-id constants.
//!
//! Kept in a sibling file so `mod.rs` stays inside the AGENTS.md 500-line limit.

use super::*;

#[test]
fn action_ids_match_documented_strings() {
    assert_eq!(ACTION_PLAYER_PLAY, "podcast.player.play");
    assert_eq!(ACTION_PLAYER_PAUSE, "podcast.player.pause");
    assert_eq!(ACTION_PLAYER_SEEK, "podcast.player.seek");
    assert_eq!(ACTION_PLAYER_SET_SPEED, "podcast.player.set_speed");
    assert_eq!(ACTION_PLAYER_SET_VOLUME, "podcast.player.set_volume");
    assert_eq!(ACTION_PLAYER_SET_SLEEP_TIMER, "podcast.player.set_sleep_timer");
    assert_eq!(ACTION_PLAYER_STOP, "podcast.player.stop");
    assert_eq!(ACTION_PLAYER_DOWNLOAD, "podcast.player.download");
    assert_eq!(ACTION_PLAYER_CANCEL_DOWNLOAD, "podcast.player.cancel_download");
    assert_eq!(ACTION_PLAYER_PAUSE_DOWNLOAD, "podcast.player.pause_download");
    assert_eq!(ACTION_PLAYER_RESUME_DOWNLOAD, "podcast.player.resume_download");
    assert_eq!(ACTION_PLAYER_CANCEL_ALL_DOWNLOADS, "podcast.player.cancel_all_downloads");
    assert_eq!(ACTION_PLAYER_SKIP_FORWARD, "podcast.player.skip_forward");
    assert_eq!(ACTION_PLAYER_SKIP_BACKWARD, "podcast.player.skip_backward");
}

#[test]
fn inbox_action_ids_match_documented_strings() {
    assert_eq!(ACTION_INBOX_TRIAGE, "podcast.inbox.triage");
    assert_eq!(ACTION_INBOX_DISMISS, "podcast.inbox.dismiss");
    assert_eq!(ACTION_INBOX_MARK_LISTENED, "podcast.inbox.mark_listened");
}

#[test]
fn download_episode_action_serde_roundtrips() {
    let a = DownloadEpisodeAction {
        episode_id: "ep-7".into(),
        url: "https://ex.com/7.mp3".into(),
    };
    let json = serde_json::to_string(&a).expect("encode");
    let decoded: DownloadEpisodeAction = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, a);
}

#[test]
fn cancel_download_action_serde_roundtrips() {
    let a = CancelDownloadAction { episode_id: "ep-7".into() };
    let json = serde_json::to_string(&a).expect("encode");
    assert_eq!(json, r#"{"episode_id":"ep-7"}"#);
    let decoded: CancelDownloadAction = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, a);
}

#[test]
fn pause_resume_download_actions_round_trip() {
    let pause = PauseDownloadAction { episode_id: "ep-7".into() };
    let resume = ResumeDownloadAction { episode_id: "ep-7".into() };
    let pj = serde_json::to_string(&pause).expect("encode");
    let rj = serde_json::to_string(&resume).expect("encode");
    let pd: PauseDownloadAction = serde_json::from_str(&pj).expect("decode");
    let rd: ResumeDownloadAction = serde_json::from_str(&rj).expect("decode");
    assert_eq!(pd, pause);
    assert_eq!(rd, resume);
}

#[test]
fn cancel_all_downloads_action_is_unit_struct() {
    let a = CancelAllDownloadsAction;
    let json = serde_json::to_string(&a).expect("encode");
    assert_eq!(json, "null");
}

#[test]
fn play_action_serde_roundtrips() {
    let a = PlayAction { episode_id: "ep-7".into() };
    let json = serde_json::to_string(&a).expect("encode");
    assert_eq!(json, r#"{"episode_id":"ep-7"}"#);
    let decoded: PlayAction = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, a);
}

#[test]
fn sleep_timer_action_handles_some_and_none() {
    let arm = SetSleepTimerAction { secs: Some(1800) };
    let json = serde_json::to_string(&arm).expect("encode");
    assert_eq!(json, r#"{"secs":1800}"#);

    let cancel = SetSleepTimerAction::default();
    let json = serde_json::to_string(&cancel).expect("encode");
    assert_eq!(json, r#"{"secs":null}"#);

    let decoded: SetSleepTimerAction = serde_json::from_str("{}").expect("decode");
    assert!(decoded.secs.is_none());
}

#[test]
fn seek_action_serde_roundtrips() {
    let a = SeekAction { position_secs: 42.5 };
    let json = serde_json::to_string(&a).expect("encode");
    let decoded: SeekAction = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, a);
}

// ── Siri / AppIntents action contract (M11) ─────────────────────────────────

#[test]
fn siri_action_ids_match_documented_strings() {
    assert_eq!(ACTION_SIRI_PLAY_LATEST, "podcast.siri.play_latest");
    assert_eq!(ACTION_SIRI_RESUME, "podcast.siri.resume");
}

#[test]
fn siri_play_latest_action_round_trips_with_podcast_id() {
    let a = SiriPlayLatestAction { podcast_id: Some("pod-1".into()) };
    let json = serde_json::to_string(&a).expect("encode");
    assert_eq!(json, r#"{"podcast_id":"pod-1"}"#);
    let decoded: SiriPlayLatestAction = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, a);
}

#[test]
fn siri_play_latest_action_omits_none_podcast_id() {
    let a = SiriPlayLatestAction::default();
    let json = serde_json::to_string(&a).expect("encode");
    assert_eq!(json, "{}");
    let decoded: SiriPlayLatestAction = serde_json::from_str("{}").expect("decode");
    assert!(decoded.podcast_id.is_none());
}

#[test]
fn siri_resume_action_is_unit_struct() {
    let a = SiriResumeAction;
    let json = serde_json::to_string(&a).expect("encode");
    assert_eq!(json, "null");
}

// ── Agent action re-export contract (M7.A) ──────────────────────────────────

#[test]
fn agent_action_ids_match_documented_strings() {
    assert_eq!(ACTION_AGENT_SEND, "podcast.agent.send");
    assert_eq!(ACTION_AGENT_APPROVE, "podcast.agent.approve");
    assert_eq!(ACTION_AGENT_DENY, "podcast.agent.deny");
    assert_eq!(ACTION_AGENT_CLEAR, "podcast.agent.clear");
}

#[test]
fn agent_send_action_round_trips_through_reexport() {
    let a = SendAgentMessageAction {
        conversation_id: Some("c1".into()),
        message: "hi".into(),
    };
    let json = serde_json::to_string(&a).expect("encode");
    let decoded: SendAgentMessageAction = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, a);
}
