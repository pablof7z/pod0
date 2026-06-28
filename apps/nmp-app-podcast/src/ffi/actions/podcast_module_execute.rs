//! Supporting types and `ActionModule` impl for the `"podcast"` namespace.
//!
//! Split from `podcast_module.rs` (AGENTS.md 500-line hard limit). The
//! `PodcastAction` enum lives in the parent; this file holds the wire-type
//! structs that appear as action fields and the `ActionModule` routing impl.

use serde::{Deserialize, Serialize};

use nmp_core::actor::ActorCommand;
use nmp_core::substrate::ActionModule;

use crate::discover_nostr::{nostr_discovery_identity, nostr_discovery_interest};

use super::PodcastAction;

/// One chapter for an [`super::PodcastAction::AddEpisode`] op. `image_url` +
/// `source_episode_id` carry the parity fields the Swift TTS composer built on
/// `Episode.Chapter` (mid-play artwork swap + source-episode chip). They round
/// the kernel store, not just the wire, so the projected chapter is identical
/// to the pre-kernel build.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct EpisodeChapterArg {
    pub start_secs: f64,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_episode_id: Option<String>,
}

/// One row in a [`super::PodcastAction::SetEpisodeTriage`] batch.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct EpisodeTriagePatch {
    pub episode_id: String,
    /// `"inbox"` | `"archived"` | `"none"` (sentinel: clear).
    pub decision: String,
    #[serde(default)]
    pub is_hero: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rationale: Option<String>,
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
        &self,
        action: Self::Action,
        correlation_id: &str,
        send: &dyn Fn(ActorCommand),
    ) -> Result<(), String> {
        // `discover_nostr` is the one `podcast.*` action that drives an
        // interest subscription rather than a host-op. NMP core owns all relay
        // connections (D7): the kernel opens the `kind:10154` subscription
        // through its own relay pool on `EnsureInterest`, and inbound shows
        // arrive via `NostrDiscoveryObserver`. Emitting an `ActorCommand`
        // requires the `send` closure, which only `execute` carries — so it
        // cannot live in the host-op handler.
        if let PodcastAction::DiscoverNostr {
            consumer_id,
            release,
        } = &action
        {
            let identity = nostr_discovery_identity(consumer_id);
            if *release {
                send(ActorCommand::DropInterestOwner(identity));
            } else {
                send(ActorCommand::EnsureInterest {
                    identity,
                    interest: nostr_discovery_interest(),
                });
            }
            return Ok(());
        }

        crate::ffi::actions::dispatch_host_op(Self::NAMESPACE, &action, correlation_id, send)
    }

    fn decode_payload(
        bytes: &[u8],
    ) -> Option<Result<Self::Action, nmp_core::substrate::ActionPayloadDecodeError>> {
        crate::action_payload::decode_podcast_payload(bytes)
    }
}
