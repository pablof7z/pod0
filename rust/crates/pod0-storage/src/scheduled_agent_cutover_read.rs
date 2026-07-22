use rusqlite::{Connection, OptionalExtension, Transaction};

use crate::{
    LegacyScheduledAgentCutoverReport, ScheduledAgentAuthorityState, ScheduledAgentCutoverState,
    StorageError,
};

type CutoverEvidenceRow = (String, i64, Vec<u8>, Vec<u8>, i64, i64, i64);

pub(super) fn read_report(
    connection: &Connection,
) -> Result<LegacyScheduledAgentCutoverReport, StorageError> {
    if let Some(report) = read_evidence(connection)? {
        return Ok(report);
    }
    match crate::scheduled_agent_store::read_authority(connection)? {
        ScheduledAgentAuthorityState::Inactive => Ok(LegacyScheduledAgentCutoverReport {
            state: ScheduledAgentCutoverState::NotStarted,
            source_fingerprint: None,
            backup_digest: None,
            backup_byte_count: None,
            task_count: 0,
            occurrence_count: 0,
        }),
        ScheduledAgentAuthorityState::Authoritative { source_generation } => {
            Ok(LegacyScheduledAgentCutoverReport {
                state: ScheduledAgentCutoverState::Authoritative { source_generation },
                source_fingerprint: None,
                backup_digest: None,
                backup_byte_count: None,
                task_count: 0,
                occurrence_count: 0,
            })
        }
    }
}

pub(super) fn read_evidence(
    connection: &Connection,
) -> Result<Option<LegacyScheduledAgentCutoverReport>, StorageError> {
    let row: Option<CutoverEvidenceRow> = connection
        .query_row(
            "SELECT state,source_generation,source_fingerprint,backup_digest,backup_byte_count,\
             task_count,occurrence_count FROM pod0_scheduled_agent_cutover_evidence WHERE singleton=1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?, row.get(6)?)),
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read scheduled-agent cutover evidence", error))?;
    row.map(decode_report).transpose()
}

pub(super) fn matching_report(
    transaction: &Transaction<'_>,
    source_generation: u64,
) -> Result<LegacyScheduledAgentCutoverReport, StorageError> {
    read_evidence(transaction)?
        .filter(|report| report.state.source_generation() == Some(source_generation))
        .ok_or(StorageError::ScheduledAgentWorkflowConflict)
}

fn decode_report(
    (state, generation, fingerprint, digest, bytes, tasks, occurrences): CutoverEvidenceRow,
) -> Result<LegacyScheduledAgentCutoverReport, StorageError> {
    let generation = unsigned(generation)?;
    let state = match state.as_str() {
        "staged" => ScheduledAgentCutoverState::Staged {
            source_generation: generation,
        },
        "verified" => ScheduledAgentCutoverState::Verified {
            source_generation: generation,
        },
        "authoritative" => ScheduledAgentCutoverState::Authoritative {
            source_generation: generation,
        },
        _ => {
            return Err(StorageError::CorruptSchema {
                detail: "scheduled-agent cutover state is malformed",
            });
        }
    };
    Ok(LegacyScheduledAgentCutoverReport {
        state,
        source_fingerprint: Some(crate::scheduled_agent_store_codec::digest(&fingerprint)?),
        backup_digest: Some(crate::scheduled_agent_store_codec::digest(&digest)?),
        backup_byte_count: Some(unsigned(bytes)?),
        task_count: count(tasks, "scheduled-agent task count is malformed")?,
        occurrence_count: count(occurrences, "scheduled-agent occurrence count is malformed")?,
    })
}

fn count(value: i64, detail: &'static str) -> Result<u32, StorageError> {
    u32::try_from(unsigned(value)?).map_err(|_| StorageError::CorruptSchema { detail })
}

fn unsigned(value: i64) -> Result<u64, StorageError> {
    u64::try_from(value).map_err(|_| StorageError::CorruptSchema {
        detail: "scheduled-agent cutover integer is malformed",
    })
}
