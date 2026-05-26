use super::*;
#[test]
fn play_action_round_trips() {
    let action = PlayerAction::Play {
        episode_id: "abc-123".into(),
    };
    let json = serde_json::to_string(&action).expect("encode");
    assert!(json.contains(r#""op":"play""#));
    assert!(json.contains(r#""episode_id":"abc-123""#));
    let decoded: PlayerAction = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, action);
}
#[test]
fn pause_stop_are_unit_variants() {
    for (action, expected_op) in [
        (PlayerAction::Pause, "pause"),
        (PlayerAction::Stop, "stop"),
    ] {
        let json = serde_json::to_string(&action).expect("encode");
        assert!(json.contains(&format!(r#""op":"{expected_op}""#)));
        let decoded: PlayerAction = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, action);
    }
}
#[test]
fn seek_encodes_position() {
    let action = PlayerAction::Seek { position_secs: 42.5 };
    let json = serde_json::to_string(&action).expect("encode");
    assert_eq!(json, r#"{"op":"seek","position_secs":42.5}"#);
    let decoded: PlayerAction = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, action);
}
#[test]
fn set_sleep_timer_handles_some_and_none() {
    let arm = PlayerAction::SetSleepTimer { secs: Some(1800) };
    let json = serde_json::to_string(&arm).expect("encode");
    assert!(json.contains("1800"));
    let decoded: PlayerAction = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, arm);
    let cancel = PlayerAction::SetSleepTimer { secs: None };
    let cancel_json = serde_json::to_string(&cancel).expect("encode");
    let decoded_cancel: PlayerAction = serde_json::from_str(&cancel_json).expect("decode");
    assert_eq!(decoded_cancel, cancel);
}
#[test]
fn enqueue_dequeue_round_trip() {
    for (action, expected_op) in [
        (PlayerAction::Enqueue { episode_id: "ep-1".into() }, "enqueue"),
        (PlayerAction::Dequeue { episode_id: "ep-1".into() }, "dequeue"),
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
    };
    let commands = std::sync::Mutex::new(Vec::<ActorCommand>::new());
    PlayerActionModule::execute(action, "corr-1", &|cmd| {
        commands.lock().unwrap().push(cmd);
    })
    .expect("execute ok");
    let commands = commands.into_inner().unwrap();
    assert_eq!(commands.len(), 1);
    let ActorCommand::DispatchHostOp { action_json, correlation_id } = &commands[0] else {
        panic!("expected DispatchHostOp");
    };
    assert_eq!(correlation_id, "corr-1");
    let v: serde_json::Value = serde_json::from_str(action_json).expect("json");
    assert_eq!(v["op"], "play");
    assert_eq!(v["episode_id"], "ep-7");
}
#[test]
fn skip_forward_round_trips() {
    let action = PlayerAction::SkipForward { secs: 30.0 };
    let json = serde_json::to_string(&action).expect("encode");
    assert_eq!(json, r#"{"op":"skip_forward","secs":30.0}"#);
    let decoded: PlayerAction = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, action);
}
#[test]
fn skip_backward_round_trips() {
    let action = PlayerAction::SkipBackward { secs: 15.0 };
    let json = serde_json::to_string(&action).expect("encode");
    assert_eq!(json, r#"{"op":"skip_backward","secs":15.0}"#);
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
        (PlayerAction::CancelDownload { episode_id: "ep-1".into() }, "cancel_download"),
        (PlayerAction::PauseDownload { episode_id: "ep-1".into() }, "pause_download"),
        (PlayerAction::ResumeDownload { episode_id: "ep-1".into() }, "resume_download"),
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
