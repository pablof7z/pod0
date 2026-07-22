use pod0_domain::TranscriptArtifact;
use rusqlite::params;

use super::authority::require_authoritative;
use super::model::{
    StoredTranscriptWorkflowStage, TranscriptCompletionInput, TranscriptWorkflowRecord,
};
use super::persist::persist_workflow;
use super::read::read_workflow;
use super::support::{next_revision, validate_time};
use crate::transcript_authority::require_transcript_authoritative;
use crate::transcript_store_codec::artifact_error;
use crate::transcript_store_read_artifact::read_artifact_by_id;
use crate::transcript_store_write_rows::{
    ensure_semantic_document, insert_or_validate_artifact, require_episode_parent,
};
use crate::{LibraryStore, StorageError};

impl LibraryStore {
    pub fn stage_transcript_workflow_completion(
        &self,
        input: TranscriptCompletionInput,
    ) -> Result<TranscriptWorkflowRecord, StorageError> {
        self.stage_transcript_workflow_completion_with_observer(input, || Ok(()))
    }

    pub(crate) fn stage_transcript_workflow_completion_with_observer<F>(
        &self,
        input: TranscriptCompletionInput,
        before_commit: F,
    ) -> Result<TranscriptWorkflowRecord, StorageError>
    where
        F: FnOnce() -> Result<(), StorageError>,
    {
        validate_time(input.observed_at_ms)?;
        let artifact = TranscriptArtifact::seal(input.artifact.clone()).map_err(artifact_error)?;
        self.write(|transaction| {
            require_authoritative(transaction)?;
            require_transcript_authoritative(transaction)?;
            let mut workflow = read_workflow(transaction, input.episode_id)?
                .ok_or(StorageError::TranscriptWorkflowNotFound)?;
            validate_completion_fence(&workflow, &input)?;
            if workflow.stage == StoredTranscriptWorkflowStage::CompletionObserved {
                return replay_completion(transaction, workflow, &artifact);
            }
            if !matches!(
                workflow.stage,
                StoredTranscriptWorkflowStage::PublisherRequested
                    | StoredTranscriptWorkflowStage::SubmissionAuthorized
                    | StoredTranscriptWorkflowStage::ProviderAccepted
            ) || input.observed_at_ms < workflow.updated_at_ms
                || artifact.episode_id != workflow.episode_id
                || artifact.source_revision != workflow.request.source_revision
            {
                return Err(StorageError::StaleTranscriptAttempt);
            }
            if let (Some(expected), Some(observed)) = (
                workflow.external_operation_id.as_deref(),
                input.external_operation_id.as_deref(),
            ) && expected != observed
            {
                return Err(StorageError::StaleTranscriptAttempt);
            }
            require_episode_parent(transaction, &artifact)?;
            ensure_semantic_document(transaction, &artifact)?;
            insert_or_validate_artifact(transaction, &artifact, None, input.observed_at_ms)?;
            workflow.stage = StoredTranscriptWorkflowStage::CompletionObserved;
            workflow.workflow_revision = next_revision(workflow.workflow_revision)?;
            workflow.completion_artifact_id = Some(artifact.artifact_id);
            workflow.external_operation_id = input
                .external_operation_id
                .or(workflow.external_operation_id);
            workflow.provider_status = input.provider_status.or(workflow.provider_status);
            workflow.updated_at_ms = input.observed_at_ms;
            persist_workflow(transaction, &workflow)?;
            update_attempt_completion(transaction, &workflow)?;
            before_commit()?;
            Ok(workflow)
        })
    }
}

fn validate_completion_fence(
    workflow: &TranscriptWorkflowRecord,
    input: &TranscriptCompletionInput,
) -> Result<(), StorageError> {
    if workflow.request_id != Some(input.request_id)
        || workflow.attempt_id != input.attempt_id
        || workflow.submission_fence_id != input.submission_fence_id
        || input.attempt_id.is_some() != input.submission_fence_id.is_some()
    {
        return Err(StorageError::StaleTranscriptAttempt);
    }
    Ok(())
}

fn replay_completion(
    transaction: &rusqlite::Transaction<'_>,
    workflow: TranscriptWorkflowRecord,
    artifact: &TranscriptArtifact,
) -> Result<TranscriptWorkflowRecord, StorageError> {
    let stored_id = workflow
        .completion_artifact_id
        .ok_or(StorageError::TranscriptWorkflowConflict)?;
    let stored = read_artifact_by_id(transaction, stored_id)?
        .ok_or(StorageError::InvalidTranscriptArtifact)?;
    if stored == *artifact {
        Ok(workflow)
    } else {
        Err(StorageError::TranscriptWorkflowConflict)
    }
}

fn update_attempt_completion(
    transaction: &rusqlite::Transaction<'_>,
    workflow: &TranscriptWorkflowRecord,
) -> Result<(), StorageError> {
    let Some(attempt_id) = workflow.attempt_id else {
        return Ok(());
    };
    let changed = transaction.execute(
        "UPDATE pod0_transcript_attempts SET state='completion_observed',completion_artifact_id=?1,
         external_operation_id=COALESCE(?2,external_operation_id),provider_status=COALESCE(?3,provider_status),
         updated_at_ms=?4 WHERE attempt_id=?5 AND state IN('authorized','provider_accepted','ambiguous')",
        params![workflow.completion_artifact_id.map(|id| id.into_bytes().to_vec()),workflow.external_operation_id,
            workflow.provider_status,workflow.updated_at_ms,attempt_id.into_bytes().as_slice()],
    ).map_err(|error| StorageError::sqlite("stage transcript attempt completion", error))?;
    if changed != 1 {
        return Err(StorageError::StaleTranscriptAttempt);
    }
    Ok(())
}
