//! Agent-chat action payloads.
//!
//! Stable string ids the iOS shell encodes alongside JSON payloads when
//! it dispatches an agent action through the kernel. The `ActionModule`
//! impls that actually mutate state arrive in M7.B; M7.A only fixes the
//! wire shape so the Swift bridge has a contract to encode against.
//!
//! ## Wire shape
//!
//! ```text
//! podcast.agent.send     — SendAgentMessageAction { conversation_id?, message }
//! podcast.agent.approve  — ApproveAction          { approval_id }
//! podcast.agent.deny     — DenyAction             { approval_id, reason? }
//! podcast.agent.clear    — ClearConversationAction{ conversation_id }
//! ```

use serde::{Deserialize, Serialize};

/// `podcast.agent.send` — append a user turn to a conversation and
/// trigger the agent loop.
pub const ACTION_AGENT_SEND: &str = "podcast.agent.send";

/// `podcast.agent.approve` — accept a pending approval.
pub const ACTION_AGENT_APPROVE: &str = "podcast.agent.approve";

/// `podcast.agent.deny` — reject a pending approval (optionally with a
/// human-readable reason).
pub const ACTION_AGENT_DENY: &str = "podcast.agent.deny";

/// `podcast.agent.clear` — wipe the turns of a conversation while
/// keeping the conversation row itself (so titles and ids stay stable).
pub const ACTION_AGENT_CLEAR: &str = "podcast.agent.clear";

/// Payload for [`ACTION_AGENT_SEND`].
///
/// `conversation_id` is optional: when absent the projection layer mints
/// a fresh conversation, returning its id on the next snapshot tick.
/// `message` is the user's literal text (UTF-8, no markdown rendering at
/// this layer).
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct SendAgentMessageAction {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conversation_id: Option<String>,
    pub message: String,
}

/// Payload for [`ACTION_AGENT_APPROVE`].
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct ApproveAction {
    pub approval_id: String,
}

/// Payload for [`ACTION_AGENT_DENY`].
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct DenyAction {
    pub approval_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Payload for [`ACTION_AGENT_CLEAR`].
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct ClearConversationAction {
    pub conversation_id: String,
}

#[cfg(test)]
#[path = "agent_tests.rs"]
mod tests;
