//! Tests for [`super::podcast_module`] — PodcastAction serde and execute routing.
//!
//! Extracted from `podcast_module.rs` to keep that file under the 500-line hard limit.

use super::*;

#[test]
fn subscribe_action_round_trips() {
    let action = PodcastAction::Subscribe {
        feed_url: "https://feeds.example.com/podcast.rss".into(),
    };
    let json = serde_json::to_string(&action).expect("encode");
    assert!(json.contains(r#""op":"subscribe""#));
    assert!(json.contains(r#""feed_url""#));
    let decoded: PodcastAction = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, action);
}

#[test]
fn import_opml_action_round_trips() {
    let xml = "<opml version=\"2.0\"><body/></opml>".to_string();
    let action = PodcastAction::ImportOpml { content: xml.clone() };
    let json = serde_json::to_string(&action).expect("encode");
    assert!(json.contains(r#""op":"import_opml""#));
    assert!(json.contains(r#""content""#));
    let decoded: PodcastAction = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, action);
}

#[test]
fn download_action_round_trips() {
    let action = PodcastAction::Download {
        episode_id: "ep-7".into(),
        url: None,
    };
    let json = serde_json::to_string(&action).expect("encode");
    assert!(json.contains(r#""op":"download""#));
    assert!(json.contains(r#""episode_id":"ep-7""#));
    let decoded: PodcastAction = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, action);
}

#[test]
fn update_settings_action_round_trips() {
    let action = PodcastAction::UpdateSettings {
        has_completed_onboarding: Some(true),
    };
    let json = serde_json::to_string(&action).expect("encode");
    assert!(json.contains(r#""op":"update_settings""#));
    assert!(json.contains(r#""has_completed_onboarding":true"#));
}

#[test]
fn set_auto_download_action_round_trips() {
    let action = PodcastAction::SetAutoDownload {
        podcast_id: "abc-123".into(),
        enabled: true,
        wifi_only: true,
    };
    let json = serde_json::to_string(&action).expect("encode");
    assert!(json.contains(r#""op":"set_auto_download""#));
    assert!(json.contains(r#""podcast_id":"abc-123""#));
    assert!(json.contains(r#""enabled":true"#));
    let decoded: PodcastAction = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, action);
}

#[test]
fn update_settings_action_omits_none_fields() {
    // Empty patch — no field overrides.
    let action = PodcastAction::UpdateSettings {
        has_completed_onboarding: None,
    };
    let json = serde_json::to_string(&action).expect("encode");
    assert!(json.contains(r#""op":"update_settings""#));
    assert!(!json.contains("has_completed_onboarding"));
}

#[test]
fn delete_download_action_round_trips() {
    let action = PodcastAction::DeleteDownload {
        episode_id: "ep-7".into(),
    };
    let json = serde_json::to_string(&action).expect("encode");
    assert!(json.contains(r#""op":"delete_download""#));
    assert!(json.contains(r#""episode_id":"ep-7""#));
    let decoded: PodcastAction = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, action);
}

#[test]
fn discover_nostr_action_round_trips() {
    let claim = PodcastAction::DiscoverNostr {
        consumer_id: "discover-view".into(),
        release: false,
    };
    let json = serde_json::to_string(&claim).expect("encode claim");
    assert!(json.contains(r#""op":"discover_nostr""#));
    assert!(json.contains(r#""consumer_id":"discover-view""#));
    let decoded: PodcastAction = serde_json::from_str(&json).expect("decode claim");
    assert_eq!(decoded, claim);

    let release = PodcastAction::DiscoverNostr {
        consumer_id: "discover-view".into(),
        release: true,
    };
    let json = serde_json::to_string(&release).expect("encode release");
    assert!(json.contains(r#""release":true"#));
}

#[test]
fn fetch_comments_action_round_trips() {
    let action = PodcastAction::FetchComments {
        episode_id: "ep-7".into(),
    };
    let json = serde_json::to_string(&action).expect("encode");
    assert!(json.contains(r#""op":"fetch_comments""#));
    assert!(json.contains(r#""episode_id":"ep-7""#));
    let decoded: PodcastAction = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, action);
}

#[test]
fn discover_nostr_action_omits_false_release() {
    let action = PodcastAction::DiscoverNostr {
        consumer_id: "v".into(),
        release: false,
    };
    let json = serde_json::to_string(&action).expect("encode");
    // `release` has serde(default) so false is omitted
    assert!(!json.contains("release") || json.contains(r#""release":false"#));
    assert!(json.contains(r#""op":"discover_nostr""#));
}

#[test]
fn post_comment_action_round_trips() {
    let action = PodcastAction::PostComment {
        episode_id: "ep-7".into(),
        content: "loved it".into(),
    };
    let json = serde_json::to_string(&action).expect("encode");
    assert!(json.contains(r#""op":"post_comment""#));
    assert!(json.contains(r#""episode_id":"ep-7""#));
    assert!(json.contains(r#""content":"loved it""#));
}

#[test]
fn fetch_contacts_action_round_trips() {
    let action = PodcastAction::FetchContacts;
    let json = serde_json::to_string(&action).expect("encode");
    // No data fields — just the discriminator.
    assert_eq!(json, r#"{"op":"fetch_contacts"}"#);
    let decoded: PodcastAction = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, action);
}

#[test]
fn execute_emits_dispatch_host_op() {
    let action = PodcastAction::Subscribe {
        feed_url: "https://feeds.example.com/podcast.rss".into(),
    };
    let commands = std::sync::Mutex::new(Vec::<ActorCommand>::new());
    PodcastActionModule::execute(action, "corr-1", &|cmd| {
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
    assert_eq!(v["op"], "subscribe");
}

#[test]
fn fetch_transcript_action_round_trips() {
    let action = PodcastAction::FetchTranscript {
        episode_id: "ep-1".into(),
    };
    let json = serde_json::to_string(&action).expect("encode");
    assert!(json.contains(r#""op":"fetch_transcript""#));
    assert!(json.contains(r#""episode_id":"ep-1""#));
    let decoded: PodcastAction = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, action);
}

#[test]
fn fetch_chapters_action_round_trips() {
    let action = PodcastAction::FetchChapters {
        episode_id: "11111111-2222-3333-4444-555555555555".into(),
    };
    let json = serde_json::to_string(&action).expect("encode");
    assert!(json.contains(r#""op":"fetch_chapters""#));
    assert!(json.contains(r#""episode_id""#));
    let decoded: PodcastAction = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, action);
}

#[test]
fn set_episode_triage_action_round_trips() {
    let action = PodcastAction::SetEpisodeTriage {
        decisions: vec![
            EpisodeTriagePatch {
                episode_id: "ep-1".into(),
                decision: "inbox".into(),
                is_hero: true,
                rationale: Some("Because relevant".into()),
            },
            EpisodeTriagePatch {
                episode_id: "ep-2".into(),
                decision: "none".into(),
                is_hero: false,
                rationale: None,
            },
        ],
    };
    let json = serde_json::to_string(&action).expect("encode");
    assert!(json.contains(r#""op":"set_episode_triage""#));
    assert!(json.contains(r#""decisions""#));
    assert!(json.contains(r#""is_hero":true"#));
    let decoded: PodcastAction = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, action);
}

#[test]
fn set_episode_triage_tolerates_absent_optional_fields() {
    // Swift omits `is_hero` (false) and `rationale` (nil) — serde defaults
    // must fill them so the decode doesn't throw.
    let json = r#"{"op":"set_episode_triage","decisions":[{"episode_id":"ep-9","decision":"archived"}]}"#;
    let decoded: PodcastAction = serde_json::from_str(json).expect("decode");
    match decoded {
        PodcastAction::SetEpisodeTriage { decisions } => {
            assert_eq!(decisions.len(), 1);
            assert_eq!(decisions[0].decision, "archived");
            assert!(!decisions[0].is_hero);
            assert_eq!(decisions[0].rationale, None);
        }
        other => panic!("wrong variant: {other:?}"),
    }
}

#[test]
fn mark_episodes_metadata_indexed_action_round_trips() {
    let action = PodcastAction::MarkEpisodesMetadataIndexed {
        episode_ids: vec!["ep-1".into(), "ep-2".into()],
    };
    let json = serde_json::to_string(&action).expect("encode");
    assert!(json.contains(r#""op":"mark_episodes_metadata_indexed""#));
    assert!(json.contains(r#""episode_ids""#));
    let decoded: PodcastAction = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, action);
}

#[test]
fn set_episode_transcript_status_action_round_trips() {
    let action = PodcastAction::SetEpisodeTranscriptStatus {
        episode_id: "ep-1".into(),
        status: "failed".into(),
        message: Some("network down".into()),
    };
    let json = serde_json::to_string(&action).expect("encode");
    assert!(json.contains(r#""op":"set_episode_transcript_status""#));
    assert!(json.contains(r#""status":"failed""#));
    assert!(json.contains(r#""message":"network down""#));
    let decoded: PodcastAction = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, action);
}

#[test]
fn set_episode_transcript_status_tolerates_absent_message() {
    let json = r#"{"op":"set_episode_transcript_status","episode_id":"ep-3","status":"transcribing"}"#;
    let decoded: PodcastAction = serde_json::from_str(json).expect("decode");
    match decoded {
        PodcastAction::SetEpisodeTranscriptStatus { status, message, .. } => {
            assert_eq!(status, "transcribing");
            assert_eq!(message, None);
        }
        other => panic!("wrong variant: {other:?}"),
    }
}
