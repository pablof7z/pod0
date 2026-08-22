use pod0_facade::{AgentMessageRole, AgentModelExecutionRequest};
use pod0_live_hosts::{ChatMessage, ChatRole};

/// Maps every message into an ordinary user/assistant/system turn — this
/// mirrors `messages()`'s existing role folding (tool results surface as a
/// user turn, errors as a system turn) rather than using `ChatRole::Tool`,
/// since that role requires a `tool_call_id` this durable projection does
/// not carry.
pub(super) fn to_chat_messages(execution: &AgentModelExecutionRequest) -> Vec<ChatMessage> {
    execution
        .messages
        .iter()
        .map(|message| {
            let (role, content) = match message.role {
                AgentMessageRole::User => (ChatRole::User, message.content.clone()),
                AgentMessageRole::Assistant => (ChatRole::Assistant, message.content.clone()),
                AgentMessageRole::Tool => {
                    (ChatRole::User, format!("[tool result]\n{}", message.content))
                }
                AgentMessageRole::Error => {
                    (ChatRole::System, format!("[error]\n{}", message.content))
                }
            };
            ChatMessage::text(role, content)
        })
        .collect()
}
