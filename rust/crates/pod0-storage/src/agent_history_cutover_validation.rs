use std::collections::BTreeSet;

use pod0_application::{
    MAX_LEGACY_AGENT_CONVERSATIONS, MAX_LEGACY_AGENT_TITLE_BYTES, MAX_LEGACY_AGENT_TURNS,
};
use rusqlite::{Connection, OptionalExtension};

use crate::agent_store_codec::decode_state;
use crate::{
    LegacyAgentHistoryCutoverInput, LegacyAgentHistoryCutoverReport, StorageError,
    agent_history_counts,
};

pub(super) fn validate_input(input: &LegacyAgentHistoryCutoverInput) -> Result<(), StorageError> {
    let (conversation_count, turn_count, message_count) =
        agent_history_counts(&input.conversations);
    if input.backup_byte_count == 0
        || input.observed_at.value() < 0
        || conversation_count > MAX_LEGACY_AGENT_CONVERSATIONS
        || turn_count > MAX_LEGACY_AGENT_CONVERSATIONS * MAX_LEGACY_AGENT_TURNS
        || message_count
            > MAX_LEGACY_AGENT_CONVERSATIONS
                * MAX_LEGACY_AGENT_TURNS
                * pod0_application::MAX_AGENT_PROJECTION_MESSAGES
    {
        return invalid("legacy agent history bounds are invalid");
    }
    let mut conversation_ids = BTreeSet::new();
    let mut turn_ids = BTreeSet::new();
    for conversation in &input.conversations {
        if conversation.title.len() > MAX_LEGACY_AGENT_TITLE_BYTES
            || conversation.created_at.value() < 0
            || conversation.updated_at.value() < conversation.created_at.value()
            || conversation.turns.is_empty()
            || conversation.turns.len() > MAX_LEGACY_AGENT_TURNS
            || !conversation_ids.insert(conversation.conversation_id.into_bytes())
        {
            return invalid("legacy agent conversation is invalid");
        }
        for turn in &conversation.turns {
            let projection = turn.state.projection();
            if projection.conversation_id != conversation.conversation_id
                || turn.created_at.value() < conversation.created_at.value()
                || projection.updated_at.value() < turn.created_at.value()
                || projection.updated_at.value() > conversation.updated_at.value()
                || !turn.state.is_valid_for_recovery()
                || !matches!(
                    projection.stage,
                    pod0_application::AgentTurnStage::Completed
                        | pod0_application::AgentTurnStage::Failed
                )
                || !turn_ids.insert(projection.turn_id.into_bytes())
            {
                return invalid("legacy agent turn is invalid");
            }
        }
    }
    Ok(())
}

pub(super) fn verify_staged(
    connection: &Connection,
    report: &LegacyAgentHistoryCutoverReport,
) -> Result<(), StorageError> {
    let conversation_count = count(connection, "pod0_agent_history_staged_conversations")?;
    let turn_count = count(connection, "pod0_agent_history_staged_turns")?;
    let orphan_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM pod0_agent_history_staged_turns t \
             LEFT JOIN pod0_agent_history_staged_conversations c \
             ON c.conversation_id=t.conversation_id WHERE c.conversation_id IS NULL",
            [],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("verify staged agent history ownership", error))?;
    if conversation_count != i64::from(report.conversation_count)
        || turn_count != i64::from(report.turn_count)
        || orphan_count != 0
    {
        return Err(StorageError::AgentTurnConflict);
    }
    let mut statement = connection
        .prepare(
            "SELECT turn_id,conversation_id,created_at_ms,updated_at_ms,state_json,state_digest \
             FROM pod0_agent_history_staged_turns ORDER BY turn_id",
        )
        .map_err(|error| {
            StorageError::sqlite("prepare staged agent history verification", error)
        })?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, Vec<u8>>(0)?,
                row.get::<_, Vec<u8>>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, Vec<u8>>(4)?,
                row.get::<_, Vec<u8>>(5)?,
            ))
        })
        .map_err(|error| StorageError::sqlite("read staged agent history", error))?;
    let mut message_count = 0_u32;
    for row in rows {
        let (turn_id, conversation_id, created_at, updated_at, state_json, state_digest) =
            row.map_err(|error| StorageError::sqlite("decode staged agent history", error))?;
        let state = decode_state(&state_json, &state_digest)?;
        let projection = state.projection();
        message_count = message_count
            .checked_add(
                u32::try_from(projection.messages.len())
                    .map_err(|_| StorageError::AgentTurnConflict)?,
            )
            .ok_or(StorageError::AgentTurnConflict)?;
        if projection.turn_id.into_bytes().as_slice() != turn_id
            || projection.conversation_id.into_bytes().as_slice() != conversation_id
            || projection.updated_at.value() != updated_at
            || created_at < 0
            || updated_at < created_at
        {
            return Err(StorageError::AgentTurnConflict);
        }
    }
    if message_count != report.message_count {
        return Err(StorageError::AgentTurnConflict);
    }
    let collision: Option<i64> = connection
        .query_row(
            "SELECT 1 FROM pod0_agent_history_staged_turns s \
             JOIN pod0_agent_turns t ON t.turn_id=s.turn_id LIMIT 1",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| StorageError::sqlite("inspect agent history collision", error))?;
    if collision.is_some() {
        return Err(StorageError::AgentTurnConflict);
    }
    Ok(())
}

fn count(connection: &Connection, table: &str) -> Result<i64, StorageError> {
    connection
        .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
            row.get(0)
        })
        .map_err(|error| StorageError::sqlite("count staged agent history", error))
}

fn invalid<T>(detail: &'static str) -> Result<T, StorageError> {
    Err(StorageError::InvalidLegacyRecord {
        entity: "agent_history",
        index: 0,
        detail,
    })
}
