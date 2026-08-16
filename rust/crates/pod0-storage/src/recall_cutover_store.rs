use pod0_application::RecallIndexCutoverHostOutcome;
use pod0_domain::{CancellationId, CommandId, StateRevision};
use rusqlite::OptionalExtension;

use crate::{LibraryStore, StorageError};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecallIndexCutoverStage {
    AwaitingHost,
    HostObserved { removed_file_count: u32 },
    Committed { removed_file_count: u32 },
    Failed,
    Cancelled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StoredRecallIndexCutoverWorkflow {
    pub command_id: CommandId,
    pub cancellation_id: CancellationId,
    pub revision: StateRevision,
    pub stage: RecallIndexCutoverStage,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecallIndexCutoverStartOutcome {
    Authorized(StoredRecallIndexCutoverWorkflow),
    AlreadyComplete,
    MissingPrerequisite,
}

impl LibraryStore {
    pub fn recall_index_cutover_workflow(
        &self,
    ) -> Result<Option<StoredRecallIndexCutoverWorkflow>, StorageError> {
        self.read(read)
    }

    pub fn start_recall_index_cutover(
        &self,
        command_id: CommandId,
        fingerprint: &str,
        cancellation_id: CancellationId,
        already_committed: bool,
        prerequisites_ready: bool,
        observed_at: pod0_domain::UnixTimestampMilliseconds,
    ) -> Result<RecallIndexCutoverStartOutcome, StorageError> {
        crate::transition_commit::commit_recall_index_cutover_start(
            self.path(),
            command_id,
            fingerprint,
            cancellation_id,
            already_committed,
            prerequisites_ready,
            observed_at,
        )
    }

    pub fn commit_recall_index_cutover_observation(
        &self,
        lease: pod0_application::PersistedEffectLeaseIdentity,
        observation: pod0_application::DurableRecallIndexCutoverHostObservation,
        committed_at: pod0_domain::UnixTimestampMilliseconds,
    ) -> Result<(StoredRecallIndexCutoverWorkflow, bool), StorageError> {
        crate::transition_commit::commit_recall_index_cutover_observation(
            self.path(),
            lease,
            observation,
            committed_at,
        )
    }

    pub fn finalize_recall_index_cutover(
        &self,
        command_id: CommandId,
        removed_file_count: u32,
        committed_at: pod0_domain::UnixTimestampMilliseconds,
    ) -> Result<StoredRecallIndexCutoverWorkflow, StorageError> {
        crate::transition_commit::commit_recall_index_cutover_finalize(
            self.path(),
            command_id,
            removed_file_count,
            committed_at,
        )
    }
}

pub(crate) fn read(
    connection: &rusqlite::Connection,
) -> Result<Option<StoredRecallIndexCutoverWorkflow>, StorageError> {
    connection
        .query_row(
            "SELECT command_id,cancellation_id,revision,stage,removed_file_count FROM \
         pod0_recall_index_cutover_workflow WHERE singleton=1",
            [],
            decode,
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read recall cutover workflow", error))
}

fn decode(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredRecallIndexCutoverWorkflow> {
    let command: Vec<u8> = row.get(0)?;
    let cancellation: Vec<u8> = row.get(1)?;
    let revision: i64 = row.get(2)?;
    let stage: String = row.get(3)?;
    let removed: Option<i64> = row.get(4)?;
    let count = || {
        removed
            .and_then(|value| u32::try_from(value).ok())
            .ok_or(rusqlite::Error::InvalidQuery)
    };
    Ok(StoredRecallIndexCutoverWorkflow {
        command_id: CommandId::from_bytes(
            command
                .try_into()
                .map_err(|_| rusqlite::Error::InvalidQuery)?,
        ),
        cancellation_id: CancellationId::from_bytes(
            cancellation
                .try_into()
                .map_err(|_| rusqlite::Error::InvalidQuery)?,
        ),
        revision: StateRevision::new(
            u64::try_from(revision).map_err(|_| rusqlite::Error::InvalidQuery)?,
        ),
        stage: match stage.as_str() {
            "awaiting_host" => RecallIndexCutoverStage::AwaitingHost,
            "host_observed" => RecallIndexCutoverStage::HostObserved {
                removed_file_count: count()?,
            },
            "committed" => RecallIndexCutoverStage::Committed {
                removed_file_count: count()?,
            },
            "failed" => RecallIndexCutoverStage::Failed,
            "cancelled" => RecallIndexCutoverStage::Cancelled,
            _ => return Err(rusqlite::Error::InvalidQuery),
        },
    })
}

pub(crate) fn observation_stage(
    outcome: &RecallIndexCutoverHostOutcome,
) -> (&'static str, Option<u32>) {
    match outcome {
        RecallIndexCutoverHostOutcome::ArtifactsRemoved { removed_file_count } => {
            ("host_observed", Some(*removed_file_count))
        }
        RecallIndexCutoverHostOutcome::Failed { .. } => ("failed", None),
        RecallIndexCutoverHostOutcome::Cancelled => ("cancelled", None),
    }
}
