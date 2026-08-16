use rusqlite::{Transaction, params};

use crate::agent_history_cutover_read::read_report;
use crate::agent_history_cutover_validation::validate_input;
use crate::agent_store_codec::{AGENT_STATE_SCHEMA_VERSION, encode_state, stage_code};
use crate::{
    LegacyAgentHistoryCutoverInput, LegacyAgentHistoryCutoverReport, LibraryStore, StorageError,
    agent_history_source_fingerprint, agent_history_source_generation,
};

pub fn inspect_legacy_agent_history_cutover(
    input: &LegacyAgentHistoryCutoverInput,
) -> Result<(pod0_domain::ContentDigest, u64), StorageError> {
    validate_input(input)?;
    let fingerprint = agent_history_source_fingerprint(input);
    Ok((fingerprint, agent_history_source_generation(fingerprint)))
}

impl LibraryStore {
    pub fn agent_history_cutover_report(
        &self,
    ) -> Result<LegacyAgentHistoryCutoverReport, StorageError> {
        self.read(read_report)
    }

    pub fn stage_legacy_agent_history_cutover(
        &self,
        input: LegacyAgentHistoryCutoverInput,
    ) -> Result<LegacyAgentHistoryCutoverReport, StorageError> {
        crate::transition_commit::commit_agent_history_cutover_stage(self.path(), input)
    }

    pub fn verify_legacy_agent_history_cutover(
        &self,
        source_generation: u64,
        observed_at: pod0_domain::UnixTimestampMilliseconds,
    ) -> Result<LegacyAgentHistoryCutoverReport, StorageError> {
        crate::transition_commit::commit_agent_history_cutover_verify(
            self.path(),
            source_generation,
            observed_at,
        )
    }

    pub fn commit_legacy_agent_history_cutover(
        &self,
        source_generation: u64,
        observed_at: pod0_domain::UnixTimestampMilliseconds,
    ) -> Result<LegacyAgentHistoryCutoverReport, StorageError> {
        crate::transition_commit::commit_agent_history_cutover_authority(
            self.path(),
            source_generation,
            observed_at,
        )
    }

    pub fn discard_staged_legacy_agent_history_cutover(
        &self,
        source_generation: u64,
    ) -> Result<bool, StorageError> {
        crate::transition_commit::commit_agent_history_cutover_discard(
            self.path(),
            source_generation,
        )
    }
}

pub(crate) fn stage_rows(
    transaction: &Transaction<'_>,
    input: &LegacyAgentHistoryCutoverInput,
) -> Result<(), StorageError> {
    for conversation in &input.conversations {
        transaction
            .execute(
                "INSERT INTO pod0_agent_history_staged_conversations(conversation_id,title,\
                 created_at_ms,updated_at_ms) VALUES(?1,?2,?3,?4)",
                params![
                    conversation.conversation_id.into_bytes().as_slice(),
                    conversation.title,
                    conversation.created_at.value(),
                    conversation.updated_at.value(),
                ],
            )
            .map_err(|error| StorageError::sqlite("stage legacy agent conversation", error))?;
        for turn in &conversation.turns {
            let projection = turn.state.projection();
            let (state_json, state_digest) = encode_state(&turn.state)?;
            transaction
                .execute(
                    "INSERT INTO pod0_agent_history_staged_turns(turn_id,conversation_id,\
                     created_at_ms,updated_at_ms,state_schema_version,state_json,state_digest)\
                     VALUES(?1,?2,?3,?4,?5,?6,?7)",
                    params![
                        projection.turn_id.into_bytes().as_slice(),
                        conversation.conversation_id.into_bytes().as_slice(),
                        turn.created_at.value(),
                        projection.updated_at.value(),
                        AGENT_STATE_SCHEMA_VERSION,
                        state_json,
                        state_digest.as_slice(),
                    ],
                )
                .map_err(|error| StorageError::sqlite("stage legacy agent turn", error))?;
        }
    }
    Ok(())
}

pub(crate) fn commit_rows(
    transaction: &Transaction<'_>,
    observed_at: i64,
) -> Result<(), StorageError> {
    transaction.execute(
        "INSERT INTO pod0_agent_conversation_metadata(conversation_id,title,source,created_at_ms,\
         updated_at_ms) SELECT conversation_id,title,'legacy_swift',created_at_ms,updated_at_ms \
         FROM pod0_agent_history_staged_conversations",
        [],
    ).map_err(|error| StorageError::sqlite("commit legacy agent conversations", error))?;
    let rows = {
        let mut statement = transaction.prepare(
            "SELECT turn_id,conversation_id,created_at_ms,updated_at_ms,state_json,state_digest \
             FROM pod0_agent_history_staged_turns ORDER BY turn_id",
        ).map_err(|error| StorageError::sqlite("prepare legacy agent commit", error))?;
        statement
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
            .map_err(|error| StorageError::sqlite("read legacy agent commit", error))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| StorageError::sqlite("decode legacy agent commit", error))?
    };
    for (turn_id, conversation_id, created_at, updated_at, state_json, state_digest) in rows {
        let state = crate::agent_store_codec::decode_state(&state_json, &state_digest)?;
        let projection = state.projection();
        transaction
            .execute(
                "INSERT INTO pod0_agent_turns(turn_id,conversation_id,state_revision,stage,\
             state_schema_version,state_json,state_digest,created_at_ms,updated_at_ms) \
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
                params![
                    turn_id.as_slice(),
                    conversation_id.as_slice(),
                    to_i64(projection.revision.value)?,
                    stage_code(projection.stage),
                    AGENT_STATE_SCHEMA_VERSION,
                    state_json.as_slice(),
                    state_digest.as_slice(),
                    created_at,
                    updated_at,
                ],
            )
            .map_err(|error| StorageError::sqlite("commit legacy agent turn", error))?;
        transaction
            .execute(
                "INSERT INTO pod0_agent_audit(turn_id,turn_revision,event_kind,state_digest,\
             observed_at_ms) VALUES(?1,?2,'recovered',?3,?4)",
                params![
                    turn_id.as_slice(),
                    to_i64(projection.revision.value)?,
                    state_digest.as_slice(),
                    observed_at
                ],
            )
            .map_err(|error| StorageError::sqlite("commit legacy agent audit", error))?;
    }
    Ok(())
}

pub(crate) fn ensure_empty_staging(transaction: &Transaction<'_>) -> Result<(), StorageError> {
    for table in [
        "pod0_agent_history_staged_conversations",
        "pod0_agent_history_staged_turns",
    ] {
        let count: i64 = transaction
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .map_err(|error| StorageError::sqlite("inspect agent history staging", error))?;
        if count != 0 {
            return Err(StorageError::AgentTurnConflict);
        }
    }
    Ok(())
}

pub(crate) fn clear_staged(transaction: &Transaction<'_>) -> Result<(), StorageError> {
    transaction
        .execute("DELETE FROM pod0_agent_history_staged_turns", [])
        .map_err(|error| StorageError::sqlite("clear staged agent turns", error))?;
    transaction
        .execute("DELETE FROM pod0_agent_history_staged_conversations", [])
        .map_err(|error| StorageError::sqlite("clear staged agent conversations", error))?;
    Ok(())
}

pub(crate) fn to_i64(value: u64) -> Result<i64, StorageError> {
    i64::try_from(value).map_err(|_| StorageError::AgentTurnConflict)
}
