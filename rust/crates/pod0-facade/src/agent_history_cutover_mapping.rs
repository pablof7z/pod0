use pod0_application::{
    AgentTurnState, LegacyAgentHistoryConversationInput, MAX_LEGACY_AGENT_CONVERSATIONS,
    validate_legacy_agent_conversation,
};
use pod0_domain::{ContentDigest, UnixTimestampMilliseconds};
use pod0_storage::{
    LegacyAgentHistoryConversation, LegacyAgentHistoryCutoverInput, LegacyAgentHistoryTurn,
    StorageError,
};

pub(super) fn cutover_input(
    backup_digest: ContentDigest,
    backup_byte_count: u64,
    conversations: Vec<LegacyAgentHistoryConversationInput>,
    observed_at: UnixTimestampMilliseconds,
) -> Result<LegacyAgentHistoryCutoverInput, StorageError> {
    if conversations.len() > MAX_LEGACY_AGENT_CONVERSATIONS {
        return invalid("too many legacy agent conversations");
    }
    let conversations = conversations
        .into_iter()
        .enumerate()
        .map(|(index, conversation)| map_conversation(index, conversation))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(LegacyAgentHistoryCutoverInput {
        backup_digest,
        backup_byte_count,
        conversations,
        observed_at,
    })
}

fn map_conversation(
    index: usize,
    input: LegacyAgentHistoryConversationInput,
) -> Result<LegacyAgentHistoryConversation, StorageError> {
    validate_legacy_agent_conversation(&input).map_err(|_| StorageError::InvalidLegacyRecord {
        entity: "agent_history_conversation",
        index: index as u32,
        detail: "legacy agent conversation failed validation",
    })?;
    let turns = input
        .turns
        .iter()
        .enumerate()
        .map(|(turn_index, turn)| {
            let state = AgentTurnState::import_legacy_history(input.conversation_id, turn)
                .map_err(|_| StorageError::InvalidLegacyRecord {
                    entity: "agent_history_turn",
                    index: turn_index as u32,
                    detail: "legacy agent turn failed validation",
                })?;
            Ok(LegacyAgentHistoryTurn {
                created_at: turn.created_at,
                state,
            })
        })
        .collect::<Result<Vec<_>, StorageError>>()?;
    Ok(LegacyAgentHistoryConversation {
        conversation_id: input.conversation_id,
        title: input.title,
        created_at: input.created_at,
        updated_at: input.updated_at,
        turns,
    })
}

fn invalid<T>(detail: &'static str) -> Result<T, StorageError> {
    Err(StorageError::InvalidLegacyRecord {
        entity: "agent_history",
        index: 0,
        detail,
    })
}
