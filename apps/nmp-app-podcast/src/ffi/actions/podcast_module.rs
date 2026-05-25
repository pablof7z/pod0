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
}
