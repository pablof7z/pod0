use rusqlite::{Transaction, params};

use crate::agent_history_cutover_read::{matching_report, read_evidence, read_report};
use crate::agent_history_cutover_validation::{validate_input, verify_staged};
use crate::agent_store_codec::{AGENT_STATE_SCHEMA_VERSION, encode_state, stage_code};
use crate::{
    AgentHistoryCutoverState, LegacyAgentHistoryCutoverInput, LegacyAgentHistoryCutoverReport,
    LibraryStore, StorageError, agent_history_counts, agent_history_source_fingerprint,
    agent_history_source_generation,
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
        validate_input(&input)?;
        let fingerprint = agent_history_source_fingerprint(&input);
        let generation = agent_history_source_generation(fingerprint);
        let (conversation_count, turn_count, message_count) =
            agent_history_counts(&input.conversations);
        self.write(|transaction| {
            if let Some(report) = read_evidence(transaction)? {
                if report.state.source_generation() == Some(generation)
                    && report.source_fingerprint == Some(fingerprint)
                    && report.backup_digest == Some(input.backup_digest)
                    && report.backup_byte_count == Some(input.backup_byte_count)
                    && report.conversation_count == conversation_count as u32
                    && report.turn_count == turn_count as u32
                    && report.message_count == message_count as u32
                {
                    return Ok(report);
                }
                return Err(StorageError::AgentTurnConflict);
            }
            ensure_empty_staging(transaction)?;
            stage_rows(transaction, &input)?;
            transaction
                .execute(
                    "INSERT INTO pod0_agent_history_cutover_evidence(singleton,state,\
                     source_generation,source_fingerprint,backup_digest,backup_byte_count,\
                     conversation_count,turn_count,message_count,staged_at_ms,verified_at_ms,\
                     committed_at_ms) VALUES(1,'staged',?1,?2,?3,?4,?5,?6,?7,?8,NULL,NULL)",
                    params![
                        to_i64(generation)?,
                        fingerprint.into_bytes().as_slice(),
                        input.backup_digest.into_bytes().as_slice(),
                        to_i64(input.backup_byte_count)?,
                        to_i64(conversation_count as u64)?,
                        to_i64(turn_count as u64)?,
                        to_i64(message_count as u64)?,
                        input.observed_at.value(),
                    ],
                )
                .map_err(|error| {
                    StorageError::sqlite("stage agent history cutover evidence", error)
                })?;
            read_evidence(transaction)?.ok_or(StorageError::AgentTurnConflict)
        })
    }

    pub fn verify_legacy_agent_history_cutover(
        &self,
        source_generation: u64,
        observed_at: pod0_domain::UnixTimestampMilliseconds,
    ) -> Result<LegacyAgentHistoryCutoverReport, StorageError> {
        self.write(|transaction| {
            let report = matching_report(transaction, source_generation)?;
            if matches!(report.state, AgentHistoryCutoverState::Authoritative { .. }) {
                return Ok(report);
            }
            verify_staged(transaction, &report)?;
            if matches!(report.state, AgentHistoryCutoverState::Staged { .. }) {
                transaction
                    .execute(
                        "UPDATE pod0_agent_history_cutover_evidence SET state='verified',\
                         verified_at_ms=?1 WHERE singleton=1 AND state='staged'",
                        [observed_at.value()],
                    )
                    .map_err(|error| StorageError::sqlite("verify agent history cutover", error))?;
            }
            read_evidence(transaction)?.ok_or(StorageError::AgentTurnConflict)
        })
    }

    pub fn commit_legacy_agent_history_cutover(
        &self,
        source_generation: u64,
        observed_at: pod0_domain::UnixTimestampMilliseconds,
    ) -> Result<LegacyAgentHistoryCutoverReport, StorageError> {
        self.write(|transaction| {
            let report = matching_report(transaction, source_generation)?;
            if matches!(report.state, AgentHistoryCutoverState::Authoritative { .. }) {
                return Ok(report);
            }
            if !matches!(report.state, AgentHistoryCutoverState::Verified { .. }) {
                return Err(StorageError::AgentTurnConflict);
            }
            verify_staged(transaction, &report)?;
            commit_rows(transaction, observed_at.value())?;
            clear_staged(transaction)?;
            transaction
                .execute(
                    "UPDATE pod0_agent_history_cutover_evidence SET state='authoritative',\
                     committed_at_ms=?1 WHERE singleton=1 AND state='verified'",
                    [observed_at.value()],
                )
                .map_err(|error| StorageError::sqlite("commit agent history cutover", error))?;
            if transaction.changes() != 1 {
                return Err(StorageError::AgentTurnConflict);
            }
            read_evidence(transaction)?.ok_or(StorageError::AgentTurnConflict)
        })
    }

    pub fn discard_staged_legacy_agent_history_cutover(
        &self,
        source_generation: u64,
    ) -> Result<bool, StorageError> {
        self.write(|transaction| {
            let Some(report) = read_evidence(transaction)? else {
                return Ok(false);
            };
            if report.state.source_generation() != Some(source_generation)
                || matches!(report.state, AgentHistoryCutoverState::Authoritative { .. })
            {
                return Err(StorageError::AgentTurnConflict);
            }
            clear_staged(transaction)?;
            transaction
                .execute(
                    "DELETE FROM pod0_agent_history_cutover_evidence WHERE singleton=1",
                    [],
                )
                .map_err(|error| {
                    StorageError::sqlite("discard staged agent history cutover", error)
                })?;
            Ok(true)
        })
    }
}

fn stage_rows(
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

fn commit_rows(transaction: &Transaction<'_>, observed_at: i64) -> Result<(), StorageError> {
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

fn ensure_empty_staging(transaction: &Transaction<'_>) -> Result<(), StorageError> {
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

fn clear_staged(transaction: &Transaction<'_>) -> Result<(), StorageError> {
    transaction
        .execute("DELETE FROM pod0_agent_history_staged_turns", [])
        .map_err(|error| StorageError::sqlite("clear staged agent turns", error))?;
    transaction
        .execute("DELETE FROM pod0_agent_history_staged_conversations", [])
        .map_err(|error| StorageError::sqlite("clear staged agent conversations", error))?;
    Ok(())
}

fn to_i64(value: u64) -> Result<i64, StorageError> {
    i64::try_from(value).map_err(|_| StorageError::AgentTurnConflict)
}
