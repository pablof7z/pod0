use pod0_domain::ContentDigest;
use rusqlite::{Connection, OptionalExtension, Transaction};

use crate::{AgentHistoryCutoverState, LegacyAgentHistoryCutoverReport, StorageError};

type EvidenceRow = (String, i64, Vec<u8>, Vec<u8>, i64, i64, i64, i64);

pub(super) fn read_report(
    connection: &Connection,
) -> Result<LegacyAgentHistoryCutoverReport, StorageError> {
    read_evidence(connection)?.map_or_else(
        || {
            Ok(LegacyAgentHistoryCutoverReport {
                state: AgentHistoryCutoverState::NotStarted,
                source_fingerprint: None,
                backup_digest: None,
                backup_byte_count: None,
                conversation_count: 0,
                turn_count: 0,
                message_count: 0,
            })
        },
        Ok,
    )
}

pub(super) fn read_evidence(
    connection: &Connection,
) -> Result<Option<LegacyAgentHistoryCutoverReport>, StorageError> {
    let row: Option<EvidenceRow> = connection
        .query_row(
            "SELECT state,source_generation,source_fingerprint,backup_digest,backup_byte_count,\
             conversation_count,turn_count,message_count \
             FROM pod0_agent_history_cutover_evidence WHERE singleton=1",
            [],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                ))
            },
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read agent history cutover evidence", error))?;
    row.map(decode_report).transpose()
}

pub(super) fn matching_report(
    transaction: &Transaction<'_>,
    source_generation: u64,
) -> Result<LegacyAgentHistoryCutoverReport, StorageError> {
    read_evidence(transaction)?
        .filter(|report| report.state.source_generation() == Some(source_generation))
        .ok_or(StorageError::AgentTurnConflict)
}

fn decode_report(
    (state, generation, fingerprint, digest, bytes, conversations, turns, messages): EvidenceRow,
) -> Result<LegacyAgentHistoryCutoverReport, StorageError> {
    let generation = unsigned(generation)?;
    let state = match state.as_str() {
        "staged" => AgentHistoryCutoverState::Staged {
            source_generation: generation,
        },
        "verified" => AgentHistoryCutoverState::Verified {
            source_generation: generation,
        },
        "authoritative" => AgentHistoryCutoverState::Authoritative {
            source_generation: generation,
        },
        _ => {
            return Err(StorageError::CorruptSchema {
                detail: "agent history cutover state is malformed",
            });
        }
    };
    Ok(LegacyAgentHistoryCutoverReport {
        state,
        source_fingerprint: Some(digest_value(&fingerprint)?),
        backup_digest: Some(digest_value(&digest)?),
        backup_byte_count: Some(unsigned(bytes)?),
        conversation_count: count(conversations)?,
        turn_count: count(turns)?,
        message_count: count(messages)?,
    })
}

fn digest_value(bytes: &[u8]) -> Result<ContentDigest, StorageError> {
    let bytes: [u8; 32] = bytes.try_into().map_err(|_| StorageError::CorruptSchema {
        detail: "agent history cutover digest is malformed",
    })?;
    Ok(ContentDigest::from_bytes(bytes))
}

fn count(value: i64) -> Result<u32, StorageError> {
    u32::try_from(unsigned(value)?).map_err(|_| StorageError::CorruptSchema {
        detail: "agent history cutover count is malformed",
    })
}

fn unsigned(value: i64) -> Result<u64, StorageError> {
    u64::try_from(value).map_err(|_| StorageError::CorruptSchema {
        detail: "agent history cutover integer is malformed",
    })
}
