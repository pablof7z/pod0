use super::*;

/// Test helper: extract `(action_json, correlation_id)` from an
/// `ActorCommand::Protocol(HostOpCommand { .. })` via its `Debug` output.
/// HostOpCommand fields are private in nmp-core; this avoids direct access.
#[cfg(test)]
#[allow(dead_code)]
fn extract_host_op_parts(cmd: &ActorCommand) -> (String, String) {
    let dbg = format!("{cmd:?}");
    // Debug fmt: Protocol(HostOpCommand { action_json: "{..}", correlation_id: "corr" })
    // The outer string delimiters are literal " in the Debug output; inner " are \".
    let jm = concat!("action_json: ", r#"""#);
    let js = dbg.find(jm).expect("action_json") + jm.len();
    let after = &dbg[js..];
    let je = after
        .find(concat!(r#"""#, ", correlation_id:"))
        .expect("json end");
    let raw = &after[..je];
    // Unescape \" → " and \\\\ → \\
    let tmp = raw.replace(r#"\\"#, "\x01BSLASH\x01");
    let action_json = tmp.replace(r#"\""#, r#"""#).replace("\x01BSLASH\x01", "\\");
    let cm = concat!("correlation_id: ", r#"""#);
    let cs = dbg.find(cm).expect("corr_id") + cm.len();
    let after_c = &dbg[cs..];
    let ce = after_c.find(concat!(r#"""#, " }")).expect("corr end");
    (action_json, after_c[..ce].to_string())
}

#[test]
fn play_action_round_trips() {
    let action = PlayerAction::Play {
        episode_id: "abc-123".into(),
        start_secs: None,
        end_secs: None,
    };
    let json = serde_json::to_string(&action).expect("encode");
    assert!(json.contains(r#""op":"play""#));
    assert!(json.contains(r#""episode_id":"abc-123""#));
    let decoded: PlayerAction = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, action);
}

#[test]
fn bounded_play_action_round_trips() {
    let action = PlayerAction::Play {
        episode_id: "abc-123".into(),
        start_secs: Some(12.5),
        end_secs: Some(42.0),
    };
    let json = serde_json::to_string(&action).expect("encode");
    assert_eq!(
        json,
        r#"{"op":"play","episode_id":"abc-123","start_secs":12.5,"end_secs":42.0}"#
    );
    let decoded: PlayerAction = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, action);
}
#[test]
fn pause_stop_are_unit_variants() {
    for (action, expected_op) in [(PlayerAction::Pause, "pause"), (PlayerAction::Stop, "stop")] {
        let json = serde_json::to_string(&action).expect("encode");
        assert!(json.contains(&format!(r#""op":"{expected_op}""#)));
        let decoded: PlayerAction = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, action);
    }
}
#[test]
fn seek_encodes_position() {
    let action = PlayerAction::Seek {
        position_secs: 42.5,
    };
    let json = serde_json::to_string(&action).expect("encode");
    assert_eq!(json, r#"{"op":"seek","position_secs":42.5}"#);
    let decoded: PlayerAction = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, action);
}
#[test]
fn set_sleep_timer_handles_some_and_none() {
    let arm = PlayerAction::SetSleepTimer {
        secs: Some(1800),
        end_of_episode: false,
    };
    let json = serde_json::to_string(&arm).expect("encode");
    assert!(json.contains("1800"));
    let decoded: PlayerAction = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, arm);
    let cancel = PlayerAction::SetSleepTimer {
        secs: None,
        end_of_episode: false,
    };
    let cancel_json = serde_json::to_string(&cancel).expect("encode");
    let decoded_cancel: PlayerAction = serde_json::from_str(&cancel_json).expect("decode");
    assert_eq!(decoded_cancel, cancel);
    let end = PlayerAction::SetSleepTimer {
        secs: None,
        end_of_episode: true,
    };
    let end_json = serde_json::to_string(&end).expect("encode");
    let decoded_end: PlayerAction = serde_json::from_str(&end_json).expect("decode");
    assert_eq!(decoded_end, end);
}
#[test]
fn enqueue_dequeue_round_trip() {
    for (action, expected_op) in [
        (
            PlayerAction::Enqueue {
                episode_id: "ep-1".into(),
            },
            "enqueue",
        ),
        (
            PlayerAction::Dequeue {
                episode_id: "ep-1".into(),
            },
            "dequeue",
        ),
    ] {
        let json = serde_json::to_string(&action).expect("encode");
        assert!(json.contains(&format!(r#""op":"{expected_op}""#)));
        assert!(json.contains(r#""episode_id":"ep-1""#));
        let decoded: PlayerAction = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, action);
    }
}
#[test]
fn clear_queue_and_play_next_are_unit_variants() {
    for (action, expected_op) in [
        (PlayerAction::ClearQueue, "clear_queue"),
        (PlayerAction::PlayNext, "play_next"),
    ] {
        let json = serde_json::to_string(&action).expect("encode");
        assert_eq!(json, format!(r#"{{"op":"{expected_op}"}}"#));
        let decoded: PlayerAction = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, action);
    }
}
#[test]
fn set_ad_segments_round_trips() {
    use podcast_core::AdKind;
    let action = PlayerAction::SetAdSegments {
        episode_id: "ep-1".into(),
        segments: vec![AdSegment::new(30.0, 60.0, AdKind::Midroll)],
    };
    let json = serde_json::to_string(&action).expect("encode");
    assert!(json.contains(r#""op":"set_ad_segments""#));
    assert!(json.contains(r#""episode_id":"ep-1""#));
    let decoded: PlayerAction = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, action);
}
#[test]
fn execute_emits_dispatch_host_op() {
    let action = PlayerAction::Play {
        episode_id: "ep-7".into(),
        start_secs: None,
        end_secs: None,
    };
    let commands = std::sync::Mutex::new(Vec::<ActorCommand>::new());
    PlayerActionModule
        .execute(
            &nmp_core::substrate::ActionContext::default(),
            action,
            "corr-1",
            &|cmd| {
                commands.lock().unwrap().push(cmd);
            },
        )
        .expect("execute ok");
    let commands = commands.into_inner().unwrap();
    assert_eq!(commands.len(), 1);
    let ActorCommand::Protocol(_) = &commands[0] else {
        panic!("expected Protocol command");
    };
    let (action_json, correlation_id) = extract_host_op_parts(&commands[0]);
    assert_eq!(correlation_id.as_str(), "corr-1");
    let v: serde_json::Value = serde_json::from_str(&action_json).expect("json");
    assert_eq!(v["ns"], "podcast.player");
    assert_eq!(v["action"]["op"], "play");
    assert_eq!(v["action"]["episode_id"], "ep-7");
}
#[test]
fn skip_forward_round_trips() {
    let action = PlayerAction::SkipForward { secs: Some(30.0) };
    let json = serde_json::to_string(&action).expect("encode");
    assert_eq!(json, r#"{"op":"skip_forward","secs":30.0}"#);
    let decoded: PlayerAction = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, action);
}
#[test]
fn skip_backward_round_trips() {
    let action = PlayerAction::SkipBackward { secs: Some(15.0) };
    let json = serde_json::to_string(&action).expect("encode");
    assert_eq!(json, r#"{"op":"skip_backward","secs":15.0}"#);
    let decoded: PlayerAction = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, action);
}

#[test]
fn skip_forward_omits_default_secs() {
    let action = PlayerAction::SkipForward { secs: None };
    let json = serde_json::to_string(&action).expect("encode");
    assert_eq!(json, r#"{"op":"skip_forward"}"#);
    let decoded: PlayerAction = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, action);
}

#[test]
fn download_action_round_trips() {
    let action = PlayerAction::Download {
        episode_id: "ep-1".into(),
        url: "https://ex.com/ep.mp3".into(),
    };
    let json = serde_json::to_string(&action).expect("encode");
    assert_eq!(
        json,
        r#"{"op":"download","episode_id":"ep-1","url":"https://ex.com/ep.mp3"}"#
    );
    let decoded: PlayerAction = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, action);
}

#[test]
fn download_control_actions_round_trip() {
    for (action, expected_op) in [
        (
            PlayerAction::CancelDownload {
                episode_id: "ep-1".into(),
            },
            "cancel_download",
        ),
        (
            PlayerAction::PauseDownload {
                episode_id: "ep-1".into(),
            },
            "pause_download",
        ),
        (
            PlayerAction::ResumeDownload {
                episode_id: "ep-1".into(),
            },
            "resume_download",
        ),
    ] {
        let json = serde_json::to_string(&action).expect("encode");
        assert!(json.contains(&format!(r#""op":"{expected_op}""#)));
        assert!(json.contains(r#""episode_id":"ep-1""#));
        let decoded: PlayerAction = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, action);
    }
}

#[test]
fn cancel_all_downloads_is_unit_variant() {
    let json = serde_json::to_string(&PlayerAction::CancelAllDownloads).expect("encode");
    assert_eq!(json, r#"{"op":"cancel_all_downloads"}"#);
    let decoded: PlayerAction = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, PlayerAction::CancelAllDownloads);
}
#[test]
fn reset_progress_round_trips() {
    let action = PlayerAction::ResetProgress {
        episode_id: "ep-1".into(),
    };
    let json = serde_json::to_string(&action).expect("encode");
    assert!(json.contains(r#""op":"reset_progress""#));
    assert!(json.contains(r#""episode_id":"ep-1""#));
    let decoded: PlayerAction = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, action);
}
