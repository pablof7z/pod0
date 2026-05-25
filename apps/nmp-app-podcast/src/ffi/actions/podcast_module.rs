//! Compound podcast ActionModule — routes all `"podcast.*"` dispatches.
//!
//! Swift encodes every podcast action as `{"op":"<variant>", ...fields}`.
//! The `#[serde(tag = "op", rename_all = "snake_case")]` discriminator maps
//! the string `op` value to the enum variant. The module's `execute` body
//! forwards the whole action as `ActorCommand::DispatchHostOp` so the
//! `PodcastHostOpHandler` (running on the actor thread) can call platform
//! capabilities without the kernel naming podcast-domain nouns (D0).

use serde::{Deserialize, Serialize};

use nmp_core::substrate::ActionModule;
use nmp_core::ActorCommand;

/// Wire enum for all `"podcast"` namespace actions.
///
/// `#[serde(tag = "op", rename_all = "snake_case")]` makes the JSON
/// discriminator the lowercase snake-case variant name:
/// `subscribe` → `{"op":"subscribe","feed_url":"..."}`.
///
/// Future actions (play, pause, seek, download, …) are added as new
/// variants here — no new ActionModule registrations needed.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum PodcastAction {
    Subscribe { feed_url: String },
    Unsubscribe { podcast_id: String },
    Refresh { podcast_id: String },
    RefreshAll,
    SearchItunes { query: String },
    /// Import an OPML 2.0 subscription list. `content` is the raw XML string
    /// (Swift reads the file on the platform side and forwards the text).
    /// The handler parses entries via `podcast_feeds::import_opml`, then
    /// fans out to `handle_subscribe` for each unique feed URL.
    ImportOpml { content: String },
    /// Begin downloading the episode's enclosure to local storage.
    ///
    /// The host op handler looks up the episode's `enclosure_url` from the
    /// `PodcastStore`, then dispatches `DownloadCommand::StartDownload` to
    /// the iOS `DownloadCapability`. The capability owns the
    /// `URLSessionDownloadTask`; once the report path wires up, `Completed`
    /// reports stamp `local_path` into the store, which the snapshot
    /// surfaces as `EpisodeSummary.download_path`.
    Download { episode_id: String },
    /// Remove a previously downloaded episode from disk and clear the
    /// kernel-side `local_path` mapping.
    DeleteDownload { episode_id: String },
    FetchTranscript { episode_id: String },
    /// Fetch and parse the Podcasting 2.0 chapters JSON for an episode.
    ///
    /// Self-gating in the handler: if the episode has no `chapters_url` or
    /// already has chapters loaded, the action is a `{"ok":true}` no-op.
    FetchChapters { episode_id: String },
    /// NIP-F4 (`kind:10154`) podcast discovery from a Nostr relay HTTP gateway.
    DiscoverNostr {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        query: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        relay_url: Option<String>,
    },
    /// Patch one or more fields on the kernel-side settings projection.
    ///
    /// All fields are `Option` so the iOS shell can patch a single setting
    /// at a time (e.g. only `has_completed_onboarding`) without round-tripping
    /// the full snapshot. `None` for a field means "leave existing value
    /// untouched" — replaces the legacy `updateSettings(Settings)` pattern
    /// which sent the full struct.
    UpdateSettings {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        has_completed_onboarding: Option<bool>,
    },
    /// Compose a fresh daily briefing on demand. No fields — the handler
    /// reads the current library snapshot and the configured schedule to
    /// pick source episodes.
    ///
    /// M9.A stub: the handler currently flips a `generating` status into
    /// the snapshot and returns `{"ok":true,"status":"generating"}`. The
    /// LLM composer + audio stitching wiring lands in M9.B; this variant
    /// reserves the action-dispatch path so the iOS button can be wired
    /// against a stable contract today.
    GenerateBriefing,
}

/// Single action module for the whole `"podcast"` namespace.
///
/// `execute` serializes the typed `PodcastAction` back to JSON and hands it
/// to the actor as `ActorCommand::DispatchHostOp`. The installed
/// `PodcastHostOpHandler` deserializes it, runs the op (HTTP capability call,
/// store write), and returns a `{"ok":true}` envelope. All policy lives in
/// the handler; the action module is pure routing.
pub struct PodcastActionModule;

impl ActionModule for PodcastActionModule {
    const NAMESPACE: &'static str = "podcast";

    type Action = PodcastAction;

    fn is_async_completing() -> bool {
        false
    }

    fn execute(
        action: Self::Action,
        correlation_id: &str,
        send: &dyn Fn(ActorCommand),
    ) -> Result<(), String> {
        let action_json =
            serde_json::to_string(&action).map_err(|e| e.to_string())?;
        send(ActorCommand::DispatchHostOp {
            action_json,
            correlation_id: correlation_id.to_owned(),
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
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
        let decoded: PodcastAction = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, action);
    }

    #[test]
    fn update_settings_action_omits_none_fields() {
        // Empty patch — no field overrides. Useful as a future-proof shape
        // for "ping with no-op settings update" if it ever becomes useful.
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
    fn generate_briefing_action_round_trips() {
        let action = PodcastAction::GenerateBriefing;
        let json = serde_json::to_string(&action).expect("encode");
        assert!(json.contains(r#""op":"generate_briefing""#));
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
}
