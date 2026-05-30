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
    let action = PodcastAction::DiscoverNostr {
        query: Some("rust".into()),
        relay_url: Some("https://api.nostr.band".into()),
    };
    let json = serde_json::to_string(&action).expect("encode");
    assert!(json.contains(r#""op":"discover_nostr""#));
    assert!(json.contains(r#""query":"rust""#));
}

#[test]
fn generate_briefing_action_round_trips() {
    let action = PodcastAction::GenerateBriefing;
    let json = serde_json::to_string(&action).expect("encode");
    assert!(json.contains(r#""op":"generate_briefing""#));
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
fn discover_nostr_action_omits_none_fields() {
    let action = PodcastAction::DiscoverNostr {
        query: None,
        relay_url: None,
    };
    let json = serde_json::to_string(&action).expect("encode");
    assert_eq!(json, r#"{"op":"discover_nostr"}"#);
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
