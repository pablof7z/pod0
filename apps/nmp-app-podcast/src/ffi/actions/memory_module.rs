//! Compound memory ActionModule — routes all `"podcast.memory.*"` dispatches.
//!
//! Agent memory (feature #33) is a flat key→value bag the AI agent and the
//! user can write to. The Rust kernel owns the durable store
//! ([`crate::store::PodcastStore::set_memory_fact`] + siblings); this module
//! routes the iOS wire shape into [`ActorCommand::DispatchHostOp`] so the
//! `PodcastHostOpHandler` can mutate the store and bump `rev`.
//!
//! Wire shape (matches the other `podcast.*` modules — `op` discriminator
//! drives the variant):
//!
//! ```text
//! podcast.memory.remember     { key: String, value: String, source: Option<String> }
//! podcast.memory.forget       { key: String }
//! podcast.memory.forget_all   {}
//! ```
//!
//! `source` defaults to `"user"` when absent so hand-rolled dispatches
//! (Settings → Add Memory) stay terse. The agent writes `source: "agent"`
//! when recording facts mid-conversation.

use serde::{Deserialize, Serialize};

use nmp_core::substrate::ActionModule;
use nmp_core::actor::ActorCommand;

/// Wire enum for all `"podcast.memory"` namespace actions.
///
/// Same shape as [`super::podcast_module::PodcastAction`]:
/// `#[serde(tag = "op", rename_all = "snake_case")]` makes the JSON
/// discriminator the lowercase snake-case variant name.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum MemoryAction {
    /// Upsert a fact. When a fact with the same key already exists it is
    /// replaced in-place (the original `id` + `created_at` are preserved
    /// by the store — see `PodcastStore::set_memory_fact`).
    Remember {
        key: String,
        value: String,
        /// `"user"` or `"agent"`. Optional on the wire so hand-rolled
        /// Settings calls stay terse; defaults to `"user"` when absent.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        source: Option<String>,
    },
    /// Delete a fact by key. Silent no-op when no fact with that key
    /// exists.
    Forget { key: String },
    /// Wipe every fact in the bag. Used by the Settings "Clear All"
    /// confirmation.
    ForgetAll,
}

/// Single action module for the whole `"podcast.memory"` namespace.
///
/// `execute` serializes the typed `MemoryAction` back to JSON and hands it
/// to the actor as `ActorCommand::DispatchHostOp`. The installed
/// `PodcastHostOpHandler` (extended in `memory_handler.rs`) deserializes
/// it, runs the op (store write), and returns a `{"ok":true}` envelope.
/// All policy lives in the handler; the action module is pure routing.
pub struct MemoryActionModule;

impl ActionModule for MemoryActionModule {
    const NAMESPACE: &'static str = "podcast.memory";

    type Action = MemoryAction;

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
#[path = "memory_module_tests.rs"]
mod tests;
