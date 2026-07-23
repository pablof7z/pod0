use pod0_domain::{
    AgentExecutionFenceId, AgentTurnId, CancellationId, ConversationId, StateRevision,
    UnixTimestampMilliseconds,
};

use crate::{
    AgentMessageProjection, AgentMessageRole, AgentToolName, AgentTurnProjection, AgentTurnStage,
    AgentTurnState, MAX_AGENT_MESSAGE_BYTES, MAX_AGENT_PROJECTION_MESSAGES,
};

pub const MAX_LEGACY_AGENT_CONVERSATIONS: usize = 50;
pub const MAX_LEGACY_AGENT_TURNS: usize = 100;
pub const MAX_LEGACY_AGENT_TITLE_BYTES: usize = 256;

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct LegacyAgentHistoryMessageInput {
    pub role: AgentMessageRole,
    pub content: String,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct LegacyAgentHistoryTurnInput {
    pub turn_id: AgentTurnId,
    pub created_at: UnixTimestampMilliseconds,
    pub updated_at: UnixTimestampMilliseconds,
    pub messages: Vec<LegacyAgentHistoryMessageInput>,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct LegacyAgentHistoryConversationInput {
    pub conversation_id: ConversationId,
    pub title: String,
    pub created_at: UnixTimestampMilliseconds,
    pub updated_at: UnixTimestampMilliseconds,
    pub turns: Vec<LegacyAgentHistoryTurnInput>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LegacyAgentHistoryImportError {
    InvalidConversation,
    InvalidTurn,
    InvalidMessage,
}

impl AgentTurnState {
    pub fn import_legacy_history(
        conversation_id: ConversationId,
        input: &LegacyAgentHistoryTurnInput,
    ) -> Result<Self, LegacyAgentHistoryImportError> {
        if input.created_at.value() < 0
            || input.updated_at.value() < input.created_at.value()
            || input.messages.is_empty()
            || input.messages.len() > MAX_AGENT_PROJECTION_MESSAGES
            || input.messages.first().map(|message| message.role) != Some(AgentMessageRole::User)
            || input
                .messages
                .iter()
                .skip(1)
                .any(|message| message.role == AgentMessageRole::User)
        {
            return Err(LegacyAgentHistoryImportError::InvalidTurn);
        }
        if input.messages.iter().any(|message| {
            message.content.is_empty() || message.content.len() > MAX_AGENT_MESSAGE_BYTES
        }) {
            return Err(LegacyAgentHistoryImportError::InvalidMessage);
        }
        let stage =
            if input.messages.last().map(|message| message.role) == Some(AgentMessageRole::Error) {
                AgentTurnStage::Failed
            } else {
                AgentTurnStage::Completed
            };
        let mut fence_bytes = input.turn_id.into_bytes();
        fence_bytes[0] ^= 0xa5;
        let mut cancellation_bytes = input.turn_id.into_bytes();
        cancellation_bytes[0] ^= 0x5a;
        let state = Self {
            projection: AgentTurnProjection {
                conversation_id,
                turn_id: input.turn_id,
                revision: StateRevision::new(1),
                stage,
                messages: input
                    .messages
                    .iter()
                    .map(|message| AgentMessageProjection {
                        role: message.role,
                        content: message.content.clone(),
                    })
                    .collect(),
                recall_evidence: Vec::new(),
                model_usage: Vec::new(),
                proposal: None,
                execution_fence_id: None,
                commit: None,
                safe_failure: None,
                updated_at: input.updated_at,
            },
            model_fence_id: AgentExecutionFenceId::from_bytes(fence_bytes),
            authorization_id: None,
            action_observation: None,
            model_reference: "legacy/import".to_owned(),
            available_tools: vec![AgentToolName::CreateNote],
            cancellation_id: CancellationId::from_bytes(cancellation_bytes),
        };
        if state.is_valid_for_recovery() {
            Ok(state)
        } else {
            Err(LegacyAgentHistoryImportError::InvalidTurn)
        }
    }
}

pub fn validate_legacy_agent_conversation(
    input: &LegacyAgentHistoryConversationInput,
) -> Result<(), LegacyAgentHistoryImportError> {
    if input.title.len() > MAX_LEGACY_AGENT_TITLE_BYTES
        || input.created_at.value() < 0
        || input.updated_at.value() < input.created_at.value()
        || input.turns.is_empty()
        || input.turns.len() > MAX_LEGACY_AGENT_TURNS
    {
        return Err(LegacyAgentHistoryImportError::InvalidConversation);
    }
    for turn in &input.turns {
        if turn.created_at.value() < input.created_at.value()
            || turn.updated_at.value() > input.updated_at.value()
        {
            return Err(LegacyAgentHistoryImportError::InvalidTurn);
        }
        AgentTurnState::import_legacy_history(input.conversation_id, turn)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imported_history_is_terminal_and_recoverable() {
        let input = LegacyAgentHistoryTurnInput {
            turn_id: AgentTurnId::from_bytes([2; 16]),
            created_at: UnixTimestampMilliseconds::new(10),
            updated_at: UnixTimestampMilliseconds::new(20),
            messages: vec![
                LegacyAgentHistoryMessageInput {
                    role: AgentMessageRole::User,
                    content: "What mattered?".into(),
                },
                LegacyAgentHistoryMessageInput {
                    role: AgentMessageRole::Assistant,
                    content: "Architecture mattered.".into(),
                },
            ],
        };
        let state =
            AgentTurnState::import_legacy_history(ConversationId::from_bytes([1; 16]), &input)
                .unwrap();
        assert_eq!(state.projection().stage, AgentTurnStage::Completed);
        assert!(state.is_valid_for_recovery());
    }

    #[test]
    fn imported_error_is_terminal_failure() {
        let input = LegacyAgentHistoryTurnInput {
            turn_id: AgentTurnId::from_bytes([3; 16]),
            created_at: UnixTimestampMilliseconds::new(10),
            updated_at: UnixTimestampMilliseconds::new(20),
            messages: vec![
                LegacyAgentHistoryMessageInput {
                    role: AgentMessageRole::User,
                    content: "Try this".into(),
                },
                LegacyAgentHistoryMessageInput {
                    role: AgentMessageRole::Error,
                    content: "Provider unavailable".into(),
                },
            ],
        };
        let state =
            AgentTurnState::import_legacy_history(ConversationId::from_bytes([1; 16]), &input)
                .unwrap();
        assert_eq!(state.projection().stage, AgentTurnStage::Failed);
    }
}
