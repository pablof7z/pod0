//! AI-wiki ActionModule — routes all `"podcast.wiki.*"` dispatches.
//!
//! Swift encodes every wiki action as `{"op":"<variant>", ...fields}`.
//! The `#[serde(tag = "op", rename_all = "snake_case")]` discriminator maps
//! the string `op` value to the enum variant. The module's `execute` body
//! forwards the whole action as `ActorCommand::DispatchHostOp` so the
//! `PodcastHostOpHandler` (running on the actor thread) can mutate the
//! `wiki_articles` / `wiki_search_results` slots on the handle without the
//! kernel naming podcast-domain nouns (D0).
//!
//! ## Scaffold scope (PR #39)
//!
//! `generate` produces a stub `WikiArticle` with a placeholder summary —
//! the iOS reader can render the full UI without real LLM synthesis. The
//! follow-up swap-in replaces only the summary-building path on the kernel
//! side; the wire shape stays stable.

use serde::{Deserialize, Serialize};

use nmp_core::substrate::ActionModule;
use nmp_core::ActorCommand;

/// Wire enum for all `"podcast.wiki"` namespace actions.
///
/// `#[serde(tag = "op", rename_all = "snake_case")]` makes the JSON
/// discriminator the lowercase snake-case variant name:
/// `generate` → `{"op":"generate","podcast_id":"...","topic":"..."}`.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum WikiAction {
    /// Create a new wiki article for `(podcast_id, topic)`.
    ///
    /// Returns `{"ok":true,"article_id":"<uuid>"}` with the freshly
    /// generated UUID so the caller can navigate straight to the new
    /// article without polling the snapshot.
    Generate { podcast_id: String, topic: String },
    /// Remove an article from `wiki_articles` by id.
    Delete { article_id: String },
    /// Filter `wiki_articles` by a case-insensitive substring match on
    /// `topic` and stash the result in `wiki_search_results`. Empty
    /// `query` clears the search results.
    Search { query: String },
}

/// Action module for the `"podcast.wiki"` namespace.
///
/// `execute` serializes the typed `WikiAction` back to JSON and hands it
/// to the actor as `ActorCommand::DispatchHostOp`. The installed
/// `PodcastHostOpHandler` deserializes it, mutates the wiki slots on the
/// handle, and returns the `{"ok":true,...}` envelope.
pub struct WikiActionModule;

impl ActionModule for WikiActionModule {
    const NAMESPACE: &'static str = "podcast.wiki";

    type Action = WikiAction;

    fn is_async_completing() -> bool {
        false
    }

    fn execute(
        action: Self::Action,
        correlation_id: &str,
        send: &dyn Fn(ActorCommand),
    ) -> Result<(), String> {
        let action_json = serde_json::to_string(&action).map_err(|e| e.to_string())?;
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
    fn generate_action_round_trips() {
        let action = WikiAction::Generate {
            podcast_id: "pod-1".into(),
            topic: "Bitcoin halvings".into(),
        };
        let json = serde_json::to_string(&action).expect("encode");
        assert!(json.contains(r#""op":"generate""#));
        assert!(json.contains(r#""podcast_id":"pod-1""#));
        assert!(json.contains(r#""topic":"Bitcoin halvings""#));
        let decoded: WikiAction = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, action);
    }

    #[test]
    fn delete_action_round_trips() {
        let action = WikiAction::Delete {
            article_id: "art-7".into(),
        };
        let json = serde_json::to_string(&action).expect("encode");
        assert!(json.contains(r#""op":"delete""#));
        assert!(json.contains(r#""article_id":"art-7""#));
        let decoded: WikiAction = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, action);
    }

    #[test]
    fn search_action_round_trips() {
        let action = WikiAction::Search {
            query: "halving".into(),
        };
        let json = serde_json::to_string(&action).expect("encode");
        assert!(json.contains(r#""op":"search""#));
        assert!(json.contains(r#""query":"halving""#));
        let decoded: WikiAction = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, action);
    }

    #[test]
    fn execute_emits_dispatch_host_op() {
        let action = WikiAction::Generate {
            podcast_id: "pod-1".into(),
            topic: "topic".into(),
        };
        let commands = std::sync::Mutex::new(Vec::<ActorCommand>::new());
        WikiActionModule::execute(action, "corr-1", &|cmd| {
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
        assert_eq!(v["op"], "generate");
    }

    #[test]
    fn namespace_is_podcast_wiki() {
        assert_eq!(WikiActionModule::NAMESPACE, "podcast.wiki");
    }
}
