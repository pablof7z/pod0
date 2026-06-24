//! Playback-queue `ActionModule` — routes all `"podcast.queue.*"` dispatches.
//!
//! Swift encodes every queue action as `{"op":"<variant>", ...fields}`.
//! The `#[serde(tag = "op", rename_all = "snake_case")]` discriminator maps
//! the string `op` value to the enum variant. The module's `execute` body
//! forwards the whole action as `ActorCommand::DispatchHostOp` so the
//! `PodcastHostOpHandler` (running on the actor thread) can mutate the
//! shared [`crate::queue::PlaybackQueue`] without the kernel naming
//! podcast-domain nouns (D0).
//!
//! ## Wire shape
//!
//! ```text
//! podcast.queue.add_next  { episode_id }   — push to the front
//! podcast.queue.add_last  { episode_id }   — push to the back
//! podcast.queue.remove    { episode_id }   — drop from anywhere
//! podcast.queue.clear     { }              — empty the queue
//! ```
//!
//! Every variant returns the canonical `{"ok": true}` envelope.

use serde::{Deserialize, Serialize};

use nmp_core::substrate::ActionModule;
use nmp_core::actor::ActorCommand;

/// Wire enum for all `"podcast.queue"` namespace actions.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum QueueAction {
    /// Push the episode onto the front of the queue ("Play Next").
    AddNext { episode_id: String },
    /// Push the episode onto the back of the queue ("Add to Queue").
    AddLast { episode_id: String },
    /// Drop the episode from anywhere in the queue.
    Remove { episode_id: String },
    /// Empty the queue.
    Clear,
}

/// Action module for the `"podcast.queue"` namespace.
///
/// `execute` serializes the typed [`QueueAction`] back to JSON and hands it
/// to the actor as [`ActorCommand::DispatchHostOp`]. The installed
/// [`crate::host_op_handler::PodcastHostOpHandler`] deserializes it, mutates
/// the [`crate::queue::PlaybackQueue`], bumps `rev`, and returns
/// `{"ok": true}`.
pub struct QueueActionModule;

impl ActionModule for QueueActionModule {
    const NAMESPACE: &'static str = "podcast.queue";

    type Action = QueueAction;

    fn is_async_completing() -> bool {
        false
    }

    fn execute(
        &self,
        action: Self::Action,
        correlation_id: &str,
        send: &dyn Fn(ActorCommand),
    ) -> Result<(), String> {
        crate::ffi::actions::dispatch_host_op(Self::NAMESPACE, &action, correlation_id, send)
    }

    fn decode_payload(
        bytes: &[u8],
    ) -> Option<Result<Self::Action, nmp_core::substrate::ActionPayloadDecodeError>> {
        crate::action_payload::decode_podcast_payload(bytes)
    }
}

#[cfg(test)]
#[path = "queue_module_tests.rs"]
mod tests;
