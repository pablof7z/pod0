//! Agent-chat `ActionModule` — routes all `"podcast.agent.*"` dispatches.
//!
//! Swift encodes every agent action as `{"op":"<variant>", ...fields}`.
//! The `#[serde(tag = "op", rename_all = "snake_case")]` discriminator maps
//! the string `op` value to the enum variant. The module's `execute` body
//! forwards the whole action as `ActorCommand::DispatchHostOp` so the
//! `PodcastHostOpHandler` (running on the actor thread) can append to the
//! shared in-memory conversation without the kernel naming podcast-domain
//! nouns (D0).
//!
//! ## Wire shape
//!
//! ```text
//! podcast.agent.send  — AgentChatAction::Send  { message: String }
//! podcast.agent.clear — AgentChatAction::Clear
//! ```
//!
//! Feature #32 is a UI scaffold: the handler appends the user message and
//! a single canned assistant reply (`"I'm thinking about your question…"`)
//! to the conversation in-memory and returns `{"ok":true}`. Real LLM
//! integration replaces the canned reply with a streaming response in a
//! follow-up PR without changing this wire shape.
//!
//! ## Naming
//!
//! `AgentChatAction` (not `AgentAction`) so it doesn't collide with
//! `podcast_agent_core::AgentAction`-style names re-exported from the
//! agent-core crate. The two surfaces are intentionally separate: this
//! module owns the single-thread chat UI; `podcast-agent-core` owns the
//! multi-conversation `ConversationActor` model that the UI scaffold
//! doesn't depend on yet.

use serde::{Deserialize, Serialize};

use nmp_core::substrate::ActionModule;
use nmp_core::actor::ActorCommand;

/// Wire enum for all `"podcast.agent"` namespace actions.
///
/// `#[serde(tag = "op", rename_all = "snake_case")]` makes the JSON
/// discriminator the lowercase snake-case variant name:
/// `send` → `{"op":"send","message":"..."}`,
/// `clear` → `{"op":"clear"}`.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum AgentChatAction {
    /// Append the user's `message` to the conversation transcript, then
    /// append a canned assistant reply. Returns `{"ok":true}`.
    Send { message: String },
    /// Wipe the in-memory transcript. Returns `{"ok":true}`. The
    /// snapshot builder keeps `agent` `Some` with an empty `messages`
    /// after this so the UI can distinguish "user explicitly cleared"
    /// from "agent never touched" (which would be `None`).
    Clear,
}

/// Action module for the `"podcast.agent"` namespace.
///
/// `execute` serializes the typed [`AgentChatAction`] back to JSON and
/// hands it to the actor as `ActorCommand::DispatchHostOp`. The installed
/// [`crate::host_op_handler::PodcastHostOpHandler`] deserializes it,
/// mutates the in-memory conversation on the handle, and returns
/// `{"ok":true}`. Pure routing; no policy in this module.
pub struct AgentActionModule;

impl ActionModule for AgentActionModule {
    const NAMESPACE: &'static str = "podcast.agent";

    type Action = AgentChatAction;

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
#[path = "agent_module_tests.rs"]
mod tests;
