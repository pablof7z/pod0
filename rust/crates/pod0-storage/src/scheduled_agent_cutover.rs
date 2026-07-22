use pod0_domain::UnixTimestampMilliseconds;
use rusqlite::{Transaction, params};

use crate::scheduled_agent_cutover_read::{matching_report, read_evidence, read_report};
use crate::scheduled_agent_cutover_stage::{clear_staged_rows, stage_rows};
use crate::scheduled_agent_cutover_validation::{validate_input, verify_staged_rows};
use crate::{
    LegacyScheduledAgentCutoverInput, LegacyScheduledAgentCutoverReport, LibraryStore,
    ScheduledAgentAuthorityState, ScheduledAgentCutoverState, ScheduledAgentStore, StorageError,
    scheduled_agent_cutover_source_fingerprint, scheduled_agent_cutover_source_generation,
};

pub fn inspect_legacy_scheduled_agent_cutover(
    input: &LegacyScheduledAgentCutoverInput,
) -> Result<(pod0_domain::ContentDigest, u64), StorageError> {
    validate_input(input)?;
    let fingerprint = scheduled_agent_cutover_source_fingerprint(input);
    Ok((
        fingerprint,
        scheduled_agent_cutover_source_generation(fingerprint),
    ))
}

impl LibraryStore {
    pub fn scheduled_agent_cutover_report(
        &self,
    ) -> Result<LegacyScheduledAgentCutoverReport, StorageError> {
        self.read(read_report)
    }

    pub fn stage_legacy_scheduled_agent_cutover(
        &self,
        input: LegacyScheduledAgentCutoverInput,
    ) -> Result<LegacyScheduledAgentCutoverReport, StorageError> {
        validate_input(&input)?;
        let fingerprint = scheduled_agent_cutover_source_fingerprint(&input);
        let generation = scheduled_agent_cutover_source_generation(fingerprint);
        self.write(|transaction| {
            require_inactive(transaction)?;
            if let Some(report) = read_evidence(transaction)? {
                if report.state.source_generation() == Some(generation)
                    && report.source_fingerprint == Some(fingerprint)
                    && report.backup_digest == Some(input.backup_digest)
                    && report.backup_byte_count == Some(input.backup_byte_count)
                    && report.task_count == input.tasks.len() as u32
                    && report.occurrence_count == input.occurrences.len() as u32
                {
                    return Ok(report);
                }
                return Err(StorageError::ScheduledAgentWorkflowConflict);
            }
            ensure_empty_target(transaction)?;
            stage_rows(transaction, &input)?;
            transaction.execute(
                "INSERT INTO pod0_scheduled_agent_cutover_evidence(singleton,state,source_generation,\
                 source_fingerprint,backup_digest,backup_byte_count,task_count,occurrence_count,\
                 staged_at_ms,verified_at_ms,committed_at_ms) \
                 VALUES(1,'staged',?1,?2,?3,?4,?5,?6,?7,NULL,NULL)",
                params![
                    to_i64(generation)?,
                    fingerprint.into_bytes().as_slice(),
                    input.backup_digest.into_bytes().as_slice(),
                    to_i64(input.backup_byte_count)?,
                    to_i64(input.tasks.len() as u64)?,
                    to_i64(input.occurrences.len() as u64)?,
                    input.observed_at.value(),
                ],
            ).map_err(|error| StorageError::sqlite("stage scheduled-agent cutover evidence", error))?;
            read_evidence(transaction)?.ok_or(StorageError::ScheduledAgentWorkflowConflict)
        })
    }

    pub fn verify_legacy_scheduled_agent_cutover(
        &self,
        source_generation: u64,
        observed_at: UnixTimestampMilliseconds,
    ) -> Result<LegacyScheduledAgentCutoverReport, StorageError> {
        self.write(|transaction| {
            require_inactive(transaction)?;
            let report = matching_report(transaction, source_generation)?;
            verify_staged_rows(transaction, &report)?;
            if matches!(report.state, ScheduledAgentCutoverState::Staged { .. }) {
                transaction
                    .execute(
                        "UPDATE pod0_scheduled_agent_cutover_evidence SET state='verified',\
                     verified_at_ms=?1 WHERE singleton=1 AND state='staged'",
                        [observed_at.value()],
                    )
                    .map_err(|error| {
                        StorageError::sqlite("verify scheduled-agent cutover", error)
                    })?;
            }
            read_evidence(transaction)?.ok_or(StorageError::ScheduledAgentWorkflowConflict)
        })
    }

    pub fn commit_legacy_scheduled_agent_cutover(
        &self,
        source_generation: u64,
        observed_at: UnixTimestampMilliseconds,
    ) -> Result<LegacyScheduledAgentCutoverReport, StorageError> {
        self.write(|transaction| {
            match crate::scheduled_agent_store::read_authority(transaction)? {
                ScheduledAgentAuthorityState::Authoritative {
                    source_generation: existing,
                } if existing == source_generation => {
                    return matching_report(transaction, existing);
                }
                ScheduledAgentAuthorityState::Authoritative { .. } => {
                    return Err(StorageError::ScheduledAgentWorkflowConflict);
                }
                ScheduledAgentAuthorityState::Inactive => {}
            }
            let report = matching_report(transaction, source_generation)?;
            if !matches!(report.state, ScheduledAgentCutoverState::Verified { .. }) {
                return Err(StorageError::ScheduledAgentWorkflowConflict);
            }
            verify_staged_rows(transaction, &report)?;
            transaction
                .execute(
                    "UPDATE pod0_scheduled_agent_authority SET state='authoritative',\
                 source_generation=?1,committed_at_ms=?2 WHERE singleton=1 AND state='inactive'",
                    params![to_i64(source_generation)?, observed_at.value()],
                )
                .map_err(|error| StorageError::sqlite("commit scheduled-agent authority", error))?;
            if transaction.changes() != 1 {
                return Err(StorageError::ScheduledAgentWorkflowConflict);
            }
            transaction
                .execute(
                    "UPDATE pod0_scheduled_agent_cutover_evidence SET state='authoritative',\
                 committed_at_ms=?1 WHERE singleton=1 AND state='verified'",
                    [observed_at.value()],
                )
                .map_err(|error| {
                    StorageError::sqlite("commit scheduled-agent cutover evidence", error)
                })?;
            if transaction.changes() != 1 {
                return Err(StorageError::ScheduledAgentWorkflowConflict);
            }
            read_evidence(transaction)?.ok_or(StorageError::ScheduledAgentWorkflowConflict)
        })
    }

    pub fn discard_staged_legacy_scheduled_agent_cutover(
        &self,
        source_generation: u64,
    ) -> Result<bool, StorageError> {
        self.write(|transaction| {
            require_inactive(transaction)?;
            let Some(report) = read_evidence(transaction)? else {
                return Ok(false);
            };
            if report.state.source_generation() != Some(source_generation) {
                return Err(StorageError::ScheduledAgentWorkflowConflict);
            }
            clear_staged_rows(transaction)?;
            transaction
                .execute(
                    "DELETE FROM pod0_scheduled_agent_cutover_evidence WHERE singleton=1",
                    [],
                )
                .map_err(|error| StorageError::sqlite("discard scheduled-agent cutover", error))?;
            Ok(true)
        })
    }

    pub fn scheduled_agent_store(&self) -> Result<ScheduledAgentStore, StorageError> {
        ScheduledAgentStore::open_authoritative(self.path())
    }
}

fn require_inactive(transaction: &Transaction<'_>) -> Result<(), StorageError> {
    if crate::scheduled_agent_store::read_authority(transaction)?
        == ScheduledAgentAuthorityState::Inactive
    {
        Ok(())
    } else {
        Err(StorageError::CutoverAlreadyAuthoritative)
    }
}

fn ensure_empty_target(transaction: &Transaction<'_>) -> Result<(), StorageError> {
    for table in [
        "pod0_scheduled_tasks",
        "pod0_scheduled_occurrences",
        "pod0_scheduled_attempts",
        "pod0_scheduled_completion_evidence",
        "pod0_generated_artifacts",
        "pod0_scheduled_command_receipts",
    ] {
        let count: i64 = transaction
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .map_err(|error| {
                StorageError::sqlite("inspect scheduled-agent cutover target", error)
            })?;
        if count != 0 {
            return Err(StorageError::ScheduledAgentWorkflowConflict);
        }
    }
    Ok(())
}

fn to_i64(value: u64) -> Result<i64, StorageError> {
    i64::try_from(value).map_err(|_| StorageError::ScheduledAgentWorkflowConflict)
}
