use pod0_domain::TranscriptWorkflowId;
use rusqlite::params;

use super::authority::require_authoritative;
use super::model::{
    StoredTranscriptWorkflowStage, TranscriptWorkflowCommitInput, TranscriptWorkflowCommitReceipt,
    TranscriptWorkflowRecord,
};
use super::persist::persist_workflow;
use super::read::read_workflow;
use super::support::{next_revision, validate_time};
use crate::transcript_store_read_artifact::read_artifact_by_id;
use crate::transcript_store_write::commit_and_select_transcript_in_transaction;
use crate::{LibraryStore, StorageError};

impl LibraryStore {
    pub fn commit_transcript_workflow(
        &self,
        input: TranscriptWorkflowCommitInput,
    ) -> Result<TranscriptWorkflowCommitReceipt, StorageError> {
        self.commit_transcript_workflow_with_observer(input, || Ok(()))
    }

    pub(crate) fn commit_transcript_workflow_with_observer<F>(
        &self,
        input: TranscriptWorkflowCommitInput,
        before_commit: F,
    ) -> Result<TranscriptWorkflowCommitReceipt, StorageError>
    where
        F: FnOnce() -> Result<(), StorageError>,
    {
        validate_time(input.completed_at_ms)?;
        if input.evidence_input_version.is_empty() || input.evidence_input_version.len() > 256 {
            return Err(StorageError::TranscriptWorkflowConflict);
        }
        self.write(|transaction| {
            require_authoritative(transaction)?;
            let mut workflow = read_workflow(transaction, input.episode_id)?
                .ok_or(StorageError::TranscriptWorkflowNotFound)?;
            if workflow.request_id != Some(input.request_id)
                || !matches!(
                    workflow.stage,
                    StoredTranscriptWorkflowStage::CompletionObserved
                        | StoredTranscriptWorkflowStage::EvidenceRequested
                )
            {
                return Err(StorageError::StaleTranscriptAttempt);
            }
            let artifact_id = workflow
                .completion_artifact_id
                .ok_or(StorageError::TranscriptWorkflowConflict)?;
            let artifact = read_artifact_by_id(transaction, artifact_id)?
                .ok_or(StorageError::InvalidTranscriptArtifact)?;
            let transcript = commit_and_select_transcript_in_transaction(
                transaction,
                workflow.command_id,
                workflow.expected_selection_revision,
                &artifact,
                input.completed_at_ms,
            )?;
            if workflow.stage == StoredTranscriptWorkflowStage::CompletionObserved {
                workflow.stage = StoredTranscriptWorkflowStage::EvidenceRequested;
                workflow.workflow_revision = next_revision(workflow.workflow_revision)?;
                workflow.committed_artifact_id = Some(transcript.artifact_id);
                workflow.committed_transcript_version_id = Some(transcript.transcript_version_id);
                workflow.committed_content_digest = Some(transcript.transcript_content_digest);
                workflow.resulting_selection_revision = Some(transcript.selection_revision);
                workflow.evidence_input_version = Some(input.evidence_input_version.clone());
                workflow.deadline_at_ms = None;
                workflow.not_before_ms = None;
                workflow.failure_code = None;
                workflow.failure_detail = None;
                workflow.failure_retryable = false;
                workflow.updated_at_ms = input.completed_at_ms;
                persist_workflow(transaction, &workflow)?;
                insert_evidence_request(transaction, &workflow, input.completed_at_ms)?;
                mark_attempt_committed(transaction, &workflow)?;
            } else if workflow.evidence_input_version.as_deref()
                != Some(input.evidence_input_version.as_str())
            {
                return Err(StorageError::TranscriptWorkflowConflict);
            }
            before_commit()?;
            Ok(TranscriptWorkflowCommitReceipt {
                workflow,
                transcript,
            })
        })
    }

    pub fn complete_transcript_evidence_request(
        &self,
        workflow_id: TranscriptWorkflowId,
        input_version: &str,
        completed_at_ms: i64,
    ) -> Result<TranscriptWorkflowRecord, StorageError> {
        self.complete_transcript_evidence_request_with_observer(
            workflow_id,
            input_version,
            completed_at_ms,
            || Ok(()),
        )
    }

    pub(crate) fn complete_transcript_evidence_request_with_observer<F>(
        &self,
        workflow_id: TranscriptWorkflowId,
        input_version: &str,
        completed_at_ms: i64,
        before_commit: F,
    ) -> Result<TranscriptWorkflowRecord, StorageError>
    where
        F: FnOnce() -> Result<(), StorageError>,
    {
        validate_time(completed_at_ms)?;
        self.write(|transaction| {
            require_authoritative(transaction)?;
            let episode: Vec<u8> = transaction
                .query_row(
                    "SELECT episode_id FROM pod0_transcript_workflows WHERE workflow_id=?1",
                    [workflow_id.into_bytes().as_slice()],
                    |row| row.get(0),
                )
                .map_err(|_| StorageError::TranscriptWorkflowNotFound)?;
            let episode_id = pod0_domain::EpisodeId::from_bytes(
                episode.try_into().map_err(|_| StorageError::TranscriptWorkflowConflict)?,
            );
            let mut workflow = read_workflow(transaction, episode_id)?
                .ok_or(StorageError::TranscriptWorkflowNotFound)?;
            if workflow.stage == StoredTranscriptWorkflowStage::Succeeded
                && workflow.evidence_input_version.as_deref() == Some(input_version)
            {
                return Ok(workflow);
            }
            if workflow.stage != StoredTranscriptWorkflowStage::EvidenceRequested
                || workflow.evidence_input_version.as_deref() != Some(input_version)
                || completed_at_ms < workflow.updated_at_ms
            {
                return Err(StorageError::TranscriptWorkflowConflict);
            }
            let changed = transaction
                .execute(
                    "UPDATE pod0_transcript_evidence_requests SET state='completed',completed_at_ms=?1
                     WHERE workflow_id=?2 AND input_version=?3 AND state='requested'",
                    params![completed_at_ms, workflow_id.into_bytes().as_slice(), input_version],
                )
                .map_err(|error| StorageError::sqlite("complete transcript evidence request", error))?;
            if changed != 1 { return Err(StorageError::TranscriptWorkflowConflict); }
            workflow.stage = StoredTranscriptWorkflowStage::Succeeded;
            workflow.workflow_revision = next_revision(workflow.workflow_revision)?;
            workflow.updated_at_ms = completed_at_ms;
            persist_workflow(transaction, &workflow)?;
            before_commit()?;
            Ok(workflow)
        })
    }
}

fn insert_evidence_request(
    transaction: &rusqlite::Transaction<'_>,
    workflow: &TranscriptWorkflowRecord,
    requested_at_ms: i64,
) -> Result<(), StorageError> {
    transaction.execute(
        "INSERT INTO pod0_transcript_evidence_requests(workflow_id,episode_id,transcript_version_id,
         content_digest,input_version,state,requested_at_ms) VALUES(?1,?2,?3,?4,?5,'requested',?6)",
        params![workflow.request.workflow_id.into_bytes().as_slice(),workflow.episode_id.into_bytes().as_slice(),
            workflow.committed_transcript_version_id.map(|id| id.into_bytes().to_vec()),
            workflow.committed_content_digest.map(|id| id.into_bytes().to_vec()),
            workflow.evidence_input_version,requested_at_ms],
    ).map_err(|error| StorageError::sqlite("enqueue transcript evidence", error))?;
    Ok(())
}

fn mark_attempt_committed(
    transaction: &rusqlite::Transaction<'_>,
    workflow: &TranscriptWorkflowRecord,
) -> Result<(), StorageError> {
    let Some(attempt_id) = workflow.attempt_id else {
        return Ok(());
    };
    let changed = transaction
        .execute(
            "UPDATE pod0_transcript_attempts SET state='committed',updated_at_ms=?1
         WHERE attempt_id=?2 AND state='completion_observed'",
            params![workflow.updated_at_ms, attempt_id.into_bytes().as_slice()],
        )
        .map_err(|error| StorageError::sqlite("commit transcript attempt", error))?;
    if changed != 1 {
        return Err(StorageError::StaleTranscriptAttempt);
    }
    Ok(())
}
