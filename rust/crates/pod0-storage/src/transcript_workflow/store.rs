use super::authority::require_authoritative;
use super::model::*;
use super::persist::{insert_prepared_attempt, persist_workflow};
use super::read::{read_page, read_workflow};
use super::support::{next_revision, validate_request, validate_time};
use crate::{LibraryStore, StorageError};

impl LibraryStore {
    pub fn transcript_workflow(
        &self,
        episode_id: pod0_domain::EpisodeId,
    ) -> Result<Option<TranscriptWorkflowRecord>, StorageError> {
        self.read(|connection| {
            require_authoritative(connection)?;
            read_workflow(connection, episode_id)
        })
    }

    pub fn transcript_workflow_page(
        &self,
        offset: u32,
        maximum_count: u16,
    ) -> Result<TranscriptWorkflowPage, StorageError> {
        self.read(|connection| {
            require_authoritative(connection)?;
            read_page(connection, offset, maximum_count)
        })
    }

    pub fn ensure_transcript_workflow(
        &self,
        input: TranscriptWorkflowEnsureInput,
    ) -> Result<TranscriptWorkflowEnsureOutcome, StorageError> {
        validate_ensure(&input)?;
        self.write(|transaction| {
            require_authoritative(transaction)?;
            let existing = read_workflow(transaction, input.episode_id)?;
            if existing
                .as_ref()
                .is_some_and(|record| replays(record, &input))
            {
                return Ok(TranscriptWorkflowEnsureOutcome::Existing(
                    existing.expect("checked existing"),
                ));
            }
            validate_replacement(existing.as_ref(), &input)?;
            if existing.is_some() {
                transaction
                    .execute(
                        "DELETE FROM pod0_transcript_workflows WHERE episode_id=?1",
                        [input.episode_id.into_bytes().as_slice()],
                    )
                    .map_err(|error| StorageError::sqlite("replace transcript workflow", error))?;
            }
            let revision = existing
                .as_ref()
                .map_or(Ok(pod0_domain::StateRevision::new(1)), |value| {
                    next_revision(value.workflow_revision)
                })?;
            let record = make_record(
                input,
                revision,
                existing.as_ref().map(|value| value.created_at_ms),
            );
            persist_workflow(transaction, &record)?;
            insert_prepared_attempt(transaction, &record)?;
            Ok(TranscriptWorkflowEnsureOutcome::Changed(record))
        })
    }
}

fn validate_ensure(input: &TranscriptWorkflowEnsureInput) -> Result<(), StorageError> {
    validate_request(&input.request)?;
    validate_time(input.now_ms)?;
    if input.max_attempts == 0
        || input
            .deadline_at_ms
            .is_some_and(|value| value < input.now_ms)
    {
        return Err(StorageError::TranscriptWorkflowConflict);
    }
    match input.stage {
        StoredTranscriptWorkflowStage::AwaitingPrerequisite => {
            if input.request_id.is_some() || input.prepared_attempt.is_some() {
                return Err(StorageError::TranscriptWorkflowConflict);
            }
        }
        StoredTranscriptWorkflowStage::PublisherRequested => {
            if input.request_id.is_none()
                || input.prepared_attempt.is_some()
                || !input.request.publisher_first
            {
                return Err(StorageError::TranscriptWorkflowConflict);
            }
        }
        StoredTranscriptWorkflowStage::Requested => {
            if input.request_id.is_none() || input.prepared_attempt.is_none() {
                return Err(StorageError::TranscriptWorkflowConflict);
            }
        }
        _ => return Err(StorageError::TranscriptWorkflowConflict),
    }
    Ok(())
}

fn validate_replacement(
    existing: Option<&TranscriptWorkflowRecord>,
    input: &TranscriptWorkflowEnsureInput,
) -> Result<(), StorageError> {
    match existing {
        None if input.expected_workflow_revision.is_none() => Ok(()),
        Some(record)
            if input.expected_workflow_revision == Some(record.workflow_revision)
                && !record.stage.protects_submission() =>
        {
            Ok(())
        }
        _ => Err(StorageError::TranscriptWorkflowConflict),
    }
}

fn replays(record: &TranscriptWorkflowRecord, input: &TranscriptWorkflowEnsureInput) -> bool {
    record.request == input.request
        && record.stage == input.stage
        && record.request_id == input.request_id
        && record.attempt_id == input.prepared_attempt.map(|value| value.attempt_id)
        && record.submission_fence_id
            == input
                .prepared_attempt
                .map(|value| value.submission_fence_id)
}

fn make_record(
    input: TranscriptWorkflowEnsureInput,
    revision: pod0_domain::StateRevision,
    created_at: Option<i64>,
) -> TranscriptWorkflowRecord {
    let attempt = input.prepared_attempt;
    TranscriptWorkflowRecord {
        episode_id: input.episode_id,
        request: input.request,
        stage: input.stage,
        workflow_revision: revision,
        attempt: attempt.map_or(0, |value| value.attempt),
        max_attempts: input.max_attempts,
        attempt_id: attempt.map(|value| value.attempt_id),
        submission_fence_id: attempt.map(|value| value.submission_fence_id),
        command_id: input.command_id,
        cancellation_id: input.cancellation_id,
        request_id: input.request_id,
        issued_revision: input.issued_revision,
        deadline_at_ms: input.deadline_at_ms,
        not_before_ms: None,
        submission_authorized_at_ms: None,
        external_operation_id: None,
        provider_status: None,
        completion_artifact_id: None,
        committed_artifact_id: None,
        committed_transcript_version_id: None,
        committed_content_digest: None,
        expected_selection_revision: input.expected_selection_revision,
        resulting_selection_revision: None,
        evidence_input_version: None,
        failure_code: None,
        failure_detail: None,
        failure_retryable: false,
        may_have_submitted: false,
        source_generation: None,
        created_at_ms: created_at.unwrap_or(input.now_ms),
        updated_at_ms: input.now_ms,
    }
}
