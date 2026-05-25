//! Compound NIP-F4 publishing ActionModule (`podcast.publish` namespace).
//!
//! Routes every NIP-F4 owned-podcast publishing op: per-podcast keypair
//! lifecycle (`create_owned_podcast` / `remove_owned_podcast`), show
//! event build (`publish_show`, kind:10154), episode event build
//! (`publish_episode`, kind:54), and the agent-side ownership claim
//! (`publish_author_claim`, kind:10064).
//!
//! All "publish" ops currently return
//! `{"ok": true, "status": "relay_pending", "event_tags": [...]}` and
//! stamp the constructed event JSON onto the handle's `publish_state`
//! map so the iOS shell can render "last built event" diagnostics. Real
//! relay-side publishing is wired through once the NMP Nostr relay
//! plumbing for per-app keypairs lands.
//!
//! ## D0 / D7
//!
//! The action module itself only routes — it serializes the typed
//! [`PublishAction`] back to JSON and emits a `DispatchHostOp`
//! `ActorCommand`. The host-op handler (running on the actor thread,
//! see [`crate::host_op_handler`]) owns the actual build + state mutation.
//! That layering matches `PodcastActionModule` / `PlayerActionModule`.

use serde::{Deserialize, Serialize};

use nmp_core::substrate::ActionModule;
use nmp_core::ActorCommand;

/// `podcast.publish.create_owned_podcast` — generate a per-podcast
/// secret key, derive the pubkey, write `owner_pubkey_hex` back onto
/// the `Podcast` row.
pub const ACTION_PUBLISH_CREATE_OWNED: &str = "podcast.publish.create_owned_podcast";

/// `podcast.publish.publish_show` — build a `kind:10154` show event
/// from the podcast row + its per-podcast keypair.
pub const ACTION_PUBLISH_PUBLISH_SHOW: &str = "podcast.publish.publish_show";

/// `podcast.publish.publish_episode` — build a `kind:54` episode event
/// from the episode row + its podcast's per-podcast keypair.
pub const ACTION_PUBLISH_PUBLISH_EPISODE: &str = "podcast.publish.publish_episode";

/// `podcast.publish.publish_author_claim` — build a `kind:10064`
/// author-claim event listing every owned podcast pubkey under the
/// supplied agent pubkey.
pub const ACTION_PUBLISH_PUBLISH_AUTHOR_CLAIM: &str = "podcast.publish.publish_author_claim";

/// `podcast.publish.remove_owned_podcast` — drop the per-podcast key
/// pair + clear `owner_pubkey_hex`.
pub const ACTION_PUBLISH_REMOVE_OWNED: &str = "podcast.publish.remove_owned_podcast";

/// Wire enum for every `podcast.publish.*` action.
///
/// `#[serde(tag = "op", rename_all = "snake_case")]` matches the
/// dispatch shape Swift already uses for `PodcastAction` /
/// `PlayerAction` — the iOS shell encodes
/// `{"op":"publish_show","podcast_id":"…"}` and the action module
/// dispatches the variant to the host-op handler.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum PublishAction {
    CreateOwnedPodcast { podcast_id: String },
    PublishShow { podcast_id: String },
    PublishEpisode { episode_id: String },
    PublishAuthorClaim { agent_pubkey_hex: String },
    RemoveOwnedPodcast { podcast_id: String },
}

/// Single action module for the `podcast.publish` namespace.
pub struct NipF4PublishModule;

impl ActionModule for NipF4PublishModule {
    const NAMESPACE: &'static str = "podcast.publish";

    type Action = PublishAction;

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
    fn action_ids_match_documented_strings() {
        assert_eq!(ACTION_PUBLISH_CREATE_OWNED, "podcast.publish.create_owned_podcast");
        assert_eq!(ACTION_PUBLISH_PUBLISH_SHOW, "podcast.publish.publish_show");
        assert_eq!(ACTION_PUBLISH_PUBLISH_EPISODE, "podcast.publish.publish_episode");
        assert_eq!(
            ACTION_PUBLISH_PUBLISH_AUTHOR_CLAIM,
            "podcast.publish.publish_author_claim"
        );
        assert_eq!(ACTION_PUBLISH_REMOVE_OWNED, "podcast.publish.remove_owned_podcast");
    }

    #[test]
    fn create_owned_podcast_round_trips() {
        let a = PublishAction::CreateOwnedPodcast {
            podcast_id: "pod-7".into(),
        };
        let json = serde_json::to_string(&a).expect("encode");
        assert!(json.contains(r#""op":"create_owned_podcast""#));
        assert!(json.contains(r#""podcast_id":"pod-7""#));
        let decoded: PublishAction = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, a);
    }

    #[test]
    fn publish_show_round_trips() {
        let a = PublishAction::PublishShow {
            podcast_id: "pod-7".into(),
        };
        let json = serde_json::to_string(&a).expect("encode");
        assert!(json.contains(r#""op":"publish_show""#));
        let decoded: PublishAction = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, a);
    }

    #[test]
    fn publish_episode_round_trips() {
        let a = PublishAction::PublishEpisode {
            episode_id: "ep-7".into(),
        };
        let json = serde_json::to_string(&a).expect("encode");
        assert!(json.contains(r#""op":"publish_episode""#));
        assert!(json.contains(r#""episode_id":"ep-7""#));
        let decoded: PublishAction = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, a);
    }

    #[test]
    fn publish_author_claim_round_trips() {
        let a = PublishAction::PublishAuthorClaim {
            agent_pubkey_hex: "deadbeef".into(),
        };
        let json = serde_json::to_string(&a).expect("encode");
        assert!(json.contains(r#""op":"publish_author_claim""#));
        assert!(json.contains(r#""agent_pubkey_hex":"deadbeef""#));
        let decoded: PublishAction = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, a);
    }

    #[test]
    fn remove_owned_podcast_round_trips() {
        let a = PublishAction::RemoveOwnedPodcast {
            podcast_id: "pod-7".into(),
        };
        let json = serde_json::to_string(&a).expect("encode");
        assert!(json.contains(r#""op":"remove_owned_podcast""#));
        let decoded: PublishAction = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, a);
    }

    #[test]
    fn execute_emits_dispatch_host_op() {
        let action = PublishAction::CreateOwnedPodcast {
            podcast_id: "pod-1".into(),
        };
        let commands = std::sync::Mutex::new(Vec::<ActorCommand>::new());
        NipF4PublishModule::execute(action, "corr-1", &|cmd| {
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
        assert_eq!(v["op"], "create_owned_podcast");
        assert_eq!(v["podcast_id"], "pod-1");
    }
}
