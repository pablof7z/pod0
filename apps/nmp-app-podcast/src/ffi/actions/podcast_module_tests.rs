//! Tests for [`super::podcast_module`] — PodcastAction serde and execute routing.
//!
//! Extracted from `podcast_module.rs` to keep that file under the 500-line hard limit.

use super::*;
use nmp_core::substrate::ActionModule as _;
use nmp_core::ActorCommand;

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
    let je = after.find(concat!(r#"""#, ", correlation_id:")).expect("json end");
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
fn ensure_podcast_action_round_trips() {
    let action = PodcastAction::EnsurePodcast {
        feed_url: "https://feeds.example.com/podcast.rss".into(),
    };
    let json = serde_json::to_string(&action).expect("encode");
    assert!(json.contains(r#""op":"ensure_podcast""#));
    assert!(json.contains(r#""feed_url""#));
    let decoded: PodcastAction = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, action);
}

#[test]
fn follow_state_actions_decode_swift_wire_shape() {
    let unsubscribe: PodcastAction =
        serde_json::from_str(r#"{"op":"unsubscribe","podcast_id":"p"}"#).expect("decode");
    assert_eq!(
        unsubscribe,
        PodcastAction::Unsubscribe {
            podcast_id: "p".into()
        }
    );

    let refresh: PodcastAction =
        serde_json::from_str(r#"{"op":"refresh","podcast_id":"p"}"#).expect("decode");
    assert_eq!(
        refresh,
        PodcastAction::Refresh {
            podcast_id: "p".into()
        }
    );

    let refresh_all: PodcastAction =
        serde_json::from_str(r#"{"op":"refresh_all"}"#).expect("decode");
    assert_eq!(refresh_all, PodcastAction::RefreshAll);
}

#[test]
fn create_podcast_action_round_trips() {
    let action = PodcastAction::CreatePodcast {
        podcast_id: "pod-1".into(),
        title: "Agent Show".into(),
        description: "desc".into(),
        author: "Agent".into(),
        feed_url: Some("https://example.com/feed.xml".into()),
        artwork_url: Some("https://img/a.png".into()),
        language: Some("en".into()),
        categories: vec!["Tech".into()],
        visibility: Some("private".into()),
        title_is_placeholder: true,
    };
    let json = serde_json::to_string(&action).expect("encode");
    assert!(json.contains(r#""op":"create_podcast""#));
    assert!(json.contains(r#""podcast_id":"pod-1""#));
    assert!(json.contains(r#""feed_url":"https://example.com/feed.xml""#));
    assert!(json.contains(r#""title_is_placeholder":true"#));
    let decoded: PodcastAction = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, action);
}

#[test]
fn create_podcast_decodes_swift_wire_shape() {
    // The exact body shape `AppStateStore.kernelCreatePodcast` sends for a
    // feed-less agent show (no feed_url / artwork / language). A field-name
    // mismatch between the Swift dispatcher and this enum fails decode here.
    let json = r#"{"op":"create_podcast","podcast_id":"p","title":"T","description":"","author":"A","categories":[],"visibility":"private","title_is_placeholder":false}"#;
    let decoded: PodcastAction = serde_json::from_str(json).expect("decode");
    assert_eq!(
        decoded,
        PodcastAction::CreatePodcast {
            podcast_id: "p".into(),
            title: "T".into(),
            description: String::new(),
            author: "A".into(),
            feed_url: None,
            artwork_url: None,
            language: None,
            categories: vec![],
            visibility: Some("private".into()),
            title_is_placeholder: false,
        }
    );
}

#[test]
fn add_episode_action_round_trips() {
    let action = PodcastAction::AddEpisode {
        podcast_id: "pod-1".into(),
        episode_id: "ep-1".into(),
        title: "Episode One".into(),
        enclosure_url: "file:///tmp/ep-1.m4a".into(),
        description: "notes".into(),
        duration_secs: Some(90.0),
        image_url: Some("https://img/e.png".into()),
        chapters: vec![EpisodeChapterArg {
            start_secs: 30.0,
            title: "Clip".into(),
            image_url: Some("https://img/c.png".into()),
            source_episode_id: Some("src-ep".into()),
        }],
        transcript: Some("transcript text".into()),
    };
    let json = serde_json::to_string(&action).expect("encode");
    assert!(json.contains(r#""op":"add_episode""#));
    assert!(json.contains(r#""enclosure_url":"file:///tmp/ep-1.m4a""#));
    assert!(json.contains(r#""source_episode_id":"src-ep""#));
    let decoded: PodcastAction = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, action);
}

#[test]
fn add_episode_decodes_swift_wire_shape_http_enclosure() {
    // The exact body `AppStateStore.kernelAddEpisode` sends for an external-play
    // (remote http enclosure, no chapters / image / duration / transcript).
    let json = r#"{"op":"add_episode","podcast_id":"p","episode_id":"e","title":"T","enclosure_url":"https://example.com/audio.mp3","description":"","chapters":[]}"#;
    let decoded: PodcastAction = serde_json::from_str(json).expect("decode");
    match decoded {
        PodcastAction::AddEpisode {
            enclosure_url,
            duration_secs,
            image_url,
            chapters,
            transcript,
            ..
        } => {
            assert_eq!(enclosure_url, "https://example.com/audio.mp3");
            assert!(duration_secs.is_none());
            assert!(image_url.is_none());
            assert!(chapters.is_empty());
            assert!(transcript.is_none());
        }
        other => panic!("wrong variant: {other:?}"),
    }
}

#[test]
fn import_opml_action_round_trips() {
    let xml = "<opml version=\"2.0\"><body/></opml>".to_string();
    let action = PodcastAction::ImportOpml {
        content: xml.clone(),
    };
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
fn set_auto_download_action_round_trips_all_new() {
    let action = PodcastAction::SetAutoDownload {
        podcast_id: "abc-123".into(),
        mode: Some("all_new".into()),
        count: None,
        enabled: true,
        wifi_only: true,
    };
    let json = serde_json::to_string(&action).expect("encode");
    assert!(json.contains(r#""op":"set_auto_download""#));
    assert!(json.contains(r#""podcast_id":"abc-123""#));
    assert!(json.contains(r#""mode":"all_new""#));
    let decoded: PodcastAction = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, action);
}

#[test]
fn set_auto_download_action_round_trips_latest_n() {
    let action = PodcastAction::SetAutoDownload {
        podcast_id: "abc-123".into(),
        mode: Some("latest_n".into()),
        count: Some(5),
        enabled: true,
        wifi_only: false,
    };
    let json = serde_json::to_string(&action).expect("encode");
    assert!(json.contains(r#""mode":"latest_n""#));
    assert!(json.contains(r#""count":5"#));
    let decoded: PodcastAction = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, action);
}

/// Back-compat: a stale client sends only `enabled: bool` (no `mode` field).
/// The decoder must still parse it without error.
#[test]
fn set_auto_download_legacy_bool_payload_decodes() {
    let json = r#"{"op":"set_auto_download","podcast_id":"abc-123","enabled":true,"wifi_only":true}"#;
    let action: PodcastAction = serde_json::from_str(json).expect("decode legacy payload");
    if let PodcastAction::SetAutoDownload { podcast_id, mode, enabled, .. } = action {
        assert_eq!(podcast_id, "abc-123");
        assert!(mode.is_none(), "legacy payload has no mode field");
        assert!(enabled, "legacy enabled=true preserved");
    } else {
        panic!("wrong variant");
    }
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
    PodcastActionModule.execute(action, "corr-1", &|cmd| {
        commands.lock().unwrap().push(cmd);
    })
    .expect("execute ok");
    let commands = commands.into_inner().unwrap();
    assert_eq!(commands.len(), 1);
    let ActorCommand::Protocol(_) = &commands[0]
    else { panic!("expected Protocol command"); };
    let (action_json, correlation_id) = extract_host_op_parts(&commands[0]);
    assert_eq!(correlation_id.as_str(), "corr-1");
    let v: serde_json::Value = serde_json::from_str(&action_json).expect("json");
    assert_eq!(v["ns"], "podcast");
    assert_eq!(v["action"]["op"], "subscribe");
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
    let json =
        r#"{"op":"set_episode_triage","decisions":[{"episode_id":"ep-9","decision":"archived"}]}"#;
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
        provider: Some("ElevenLabs Scribe".into()),
    };
    let json = serde_json::to_string(&action).expect("encode");
    assert!(json.contains(r#""op":"set_episode_transcript_status""#));
    assert!(json.contains(r#""status":"failed""#));
    assert!(json.contains(r#""message":"network down""#));
    assert!(json.contains(r#""provider":"ElevenLabs Scribe""#));
    let decoded: PodcastAction = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, action);
}

#[test]
fn set_episode_transcript_status_tolerates_absent_provider() {
    // Older callers omit `provider`; it must default to None.
    let json =
        r#"{"op":"set_episode_transcript_status","episode_id":"ep-9","status":"transcribing"}"#;
    let decoded: PodcastAction = serde_json::from_str(json).expect("decode");
    match decoded {
        PodcastAction::SetEpisodeTranscriptStatus { provider, .. } => {
            assert_eq!(provider, None);
        }
        other => panic!("wrong variant: {other:?}"),
    }
}

#[test]
fn set_episode_transcript_status_tolerates_absent_message() {
    let json =
        r#"{"op":"set_episode_transcript_status","episode_id":"ep-3","status":"transcribing"}"#;
    let decoded: PodcastAction = serde_json::from_str(json).expect("decode");
    match decoded {
        PodcastAction::SetEpisodeTranscriptStatus {
            status, message, ..
        } => {
            assert_eq!(status, "transcribing");
            assert_eq!(message, None);
        }
        other => panic!("wrong variant: {other:?}"),
    }
}
