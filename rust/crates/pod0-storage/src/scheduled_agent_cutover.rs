use pod0_domain::UnixTimestampMilliseconds;
use rusqlite::Transaction;

use crate::scheduled_agent_cutover_read::read_report;
use crate::scheduled_agent_cutover_validation::validate_input;
use crate::{
    LegacyScheduledAgentCutoverInput, LegacyScheduledAgentCutoverReport, LibraryStore,
    ScheduledAgentAuthorityState, ScheduledAgentStore, StorageError,
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
        crate::transition_commit::commit_scheduled_agent_cutover_stage(self.path(), input)
    }

    pub fn verify_legacy_scheduled_agent_cutover(
        &self,
        source_generation: u64,
        observed_at: UnixTimestampMilliseconds,
    ) -> Result<LegacyScheduledAgentCutoverReport, StorageError> {
        crate::transition_commit::commit_scheduled_agent_cutover_verify(
            self.path(),
            source_generation,
            observed_at,
        )
    }

    pub fn commit_legacy_scheduled_agent_cutover(
        &self,
        source_generation: u64,
        observed_at: UnixTimestampMilliseconds,
    ) -> Result<LegacyScheduledAgentCutoverReport, StorageError> {
        crate::transition_commit::commit_scheduled_agent_cutover_authority(
            self.path(),
            source_generation,
            observed_at,
        )
    }

    pub fn discard_staged_legacy_scheduled_agent_cutover(
        &self,
        source_generation: u64,
    ) -> Result<bool, StorageError> {
        crate::transition_commit::commit_scheduled_agent_cutover_discard(
            self.path(),
            source_generation,
        )
    }

    pub fn scheduled_agent_store(&self) -> Result<ScheduledAgentStore, StorageError> {
        ScheduledAgentStore::open_authoritative(self.path())
    }
}

pub(crate) fn require_inactive(transaction: &Transaction<'_>) -> Result<(), StorageError> {
    if crate::scheduled_agent_store::read_authority(transaction)?
        == ScheduledAgentAuthorityState::Inactive
    {
        Ok(())
    } else {
        Err(StorageError::CutoverAlreadyAuthoritative)
    }
}

pub(crate) fn ensure_empty_target(transaction: &Transaction<'_>) -> Result<(), StorageError> {
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

pub(crate) fn to_i64(value: u64) -> Result<i64, StorageError> {
    i64::try_from(value).map_err(|_| StorageError::ScheduledAgentWorkflowConflict)
}
