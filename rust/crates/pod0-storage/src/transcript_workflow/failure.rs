use rusqlite::params;

use super::authority::require_authoritative;
use super::model::{
    StoredTranscriptWorkflowStage, TranscriptWorkflowFailureDisposition,
    TranscriptWorkflowFailureInput, TranscriptWorkflowRecord,
};
use super::persist::{insert_prepared_attempt, persist_workflow};
use super::read::read_workflow;
use super::support::{next_revision, validate_detail, validate_time};
use crate::{LibraryStore, StorageError};

impl LibraryStore {
    pub fn fail_transcript_workflow(
        &self,
        input: TranscriptWorkflowFailureInput,
    ) -> Result<TranscriptWorkflowRecord, StorageError> {
        validate_failure(&input)?;
        self.write(|transaction| {
            require_authoritative(transaction)?;
            let mut record = read_workflow(transaction, input.episode_id)?
                .ok_or(StorageError::TranscriptWorkflowNotFound)?;
            validate_fence(&record, &input)?;
            if !matches!(
                record.stage,
                StoredTranscriptWorkflowStage::Requested
                    | StoredTranscriptWorkflowStage::PublisherRequested
                    | StoredTranscriptWorkflowStage::RetryScheduled
                    | StoredTranscriptWorkflowStage::SubmissionAuthorized
                    | StoredTranscriptWorkflowStage::ProviderAccepted
                    | StoredTranscriptWorkflowStage::CompletionObserved
            ) {
                return Err(StorageError::TranscriptWorkflowConflict);
            }
            record.workflow_revision = next_revision(record.workflow_revision)?;
            record.failure_code = Some(input.failure_code);
            record.failure_detail = input.failure_detail;
            record.failure_retryable = input.retryable;
            record.may_have_submitted |= input.may_have_submitted;
            record.updated_at_ms = input.observed_at_ms;
            let failed_attempt_id = record.attempt_id;
            apply_disposition(transaction, &mut record, input.disposition)?;
            persist_workflow(transaction, &record)?;
            update_attempt_failure(transaction, &record, failed_attempt_id)?;
            Ok(record)
        })
    }

    pub fn cancel_transcript_workflow(
        &self,
        episode_id: pod0_domain::EpisodeId,
        expected_revision: pod0_domain::StateRevision,
        observed_at_ms: i64,
    ) -> Result<TranscriptWorkflowRecord, StorageError> {
        validate_time(observed_at_ms)?;
        self.write(|transaction| {
            require_authoritative(transaction)?;
            let mut record = read_workflow(transaction, episode_id)?
                .ok_or(StorageError::TranscriptWorkflowNotFound)?;
            if record.workflow_revision != expected_revision
                || matches!(record.stage, StoredTranscriptWorkflowStage::Succeeded | StoredTranscriptWorkflowStage::Cancelled)
            {
                return Err(StorageError::TranscriptWorkflowConflict);
            }
            record.stage = StoredTranscriptWorkflowStage::Cancelled;
            record.workflow_revision = next_revision(record.workflow_revision)?;
            record.deadline_at_ms = None;
            record.not_before_ms = None;
            record.failure_code = Some("cancelled".to_owned());
            record.failure_detail = None;
            record.failure_retryable = false;
            record.updated_at_ms = observed_at_ms;
            persist_workflow(transaction, &record)?;
            if let Some(attempt_id) = record.attempt_id {
                transaction.execute(
                    "UPDATE pod0_transcript_attempts SET state='cancelled',failure_code='cancelled',
                     may_have_submitted=?1,updated_at_ms=?2 WHERE attempt_id=?3 AND state!='committed'",
                    params![i64::from(record.may_have_submitted),observed_at_ms,attempt_id.into_bytes().as_slice()],
                ).map_err(|error| StorageError::sqlite("cancel transcript attempt", error))?;
            }
            Ok(record)
        })
    }
}

fn validate_failure(input: &TranscriptWorkflowFailureInput) -> Result<(), StorageError> {
    validate_time(input.observed_at_ms)?;
    validate_detail(input.failure_detail.as_deref())?;
    if input.failure_code.is_empty() || input.failure_code.len() > 256 {
        return Err(StorageError::TranscriptWorkflowConflict);
    }
    Ok(())
}

fn validate_fence(
    record: &TranscriptWorkflowRecord,
    input: &TranscriptWorkflowFailureInput,
) -> Result<(), StorageError> {
    if record.request_id != Some(input.request_id)
        || record.attempt_id != input.attempt_id
        || record.submission_fence_id != input.submission_fence_id
        || input.attempt_id.is_some() != input.submission_fence_id.is_some()
        || input.observed_at_ms < record.updated_at_ms
    {
        return Err(StorageError::StaleTranscriptAttempt);
    }
    Ok(())
}

fn apply_disposition(
    transaction: &rusqlite::Transaction<'_>,
    record: &mut TranscriptWorkflowRecord,
    disposition: TranscriptWorkflowFailureDisposition,
) -> Result<(), StorageError> {
    match disposition {
        TranscriptWorkflowFailureDisposition::Retry {
            attempt,
            request_id,
            issued_revision,
            not_before_ms,
            deadline_at_ms,
            evidence_permits_resubmission,
        } => {
            if not_before_ms < record.updated_at_ms
                || deadline_at_ms < not_before_ms
                || attempt.attempt <= record.attempt
                || attempt.attempt > record.max_attempts
                || record.may_have_submitted
                || !evidence_permits_resubmission
            {
                return Err(StorageError::TranscriptWorkflowConflict);
            }
            record.stage = StoredTranscriptWorkflowStage::RetryScheduled;
            record.attempt = attempt.attempt;
            record.attempt_id = Some(attempt.attempt_id);
            record.submission_fence_id = Some(attempt.submission_fence_id);
            record.request_id = Some(request_id);
            record.issued_revision = issued_revision;
            record.not_before_ms = Some(not_before_ms);
            record.deadline_at_ms = Some(deadline_at_ms);
            record.submission_authorized_at_ms = None;
            record.external_operation_id = None;
            record.provider_status = None;
            record.completion_artifact_id = None;
            record.may_have_submitted = false;
            insert_prepared_attempt(transaction, record)
        }
        TranscriptWorkflowFailureDisposition::Replan => {
            terminal(record, StoredTranscriptWorkflowStage::Blocked)
        }
        TranscriptWorkflowFailureDisposition::RecoverPersisted => {
            if !record.may_have_submitted || record.external_operation_id.is_none() {
                return Err(StorageError::TranscriptWorkflowConflict);
            }
            record.stage = StoredTranscriptWorkflowStage::ProviderAccepted;
            record.deadline_at_ms = None;
            record.not_before_ms = None;
            Ok(())
        }
        TranscriptWorkflowFailureDisposition::Block => {
            terminal(record, StoredTranscriptWorkflowStage::Blocked)
        }
        TranscriptWorkflowFailureDisposition::Fail => {
            terminal(record, StoredTranscriptWorkflowStage::Failed)
        }
        TranscriptWorkflowFailureDisposition::Ambiguous => {
            record.stage = StoredTranscriptWorkflowStage::Blocked;
            record.may_have_submitted = true;
            record.failure_retryable = false;
            record.deadline_at_ms = None;
            record.not_before_ms = None;
            Ok(())
        }
        TranscriptWorkflowFailureDisposition::Cancel => {
            terminal(record, StoredTranscriptWorkflowStage::Cancelled)
        }
    }
}

fn terminal(
    record: &mut TranscriptWorkflowRecord,
    stage: StoredTranscriptWorkflowStage,
) -> Result<(), StorageError> {
    record.stage = stage;
    record.deadline_at_ms = None;
    record.not_before_ms = None;
    Ok(())
}

fn update_attempt_failure(
    transaction: &rusqlite::Transaction<'_>,
    record: &TranscriptWorkflowRecord,
    attempt_id: Option<pod0_domain::TranscriptAttemptId>,
) -> Result<(), StorageError> {
    let Some(attempt_id) = attempt_id else {
        return Ok(());
    };
    let state = match record.stage {
        StoredTranscriptWorkflowStage::RetryScheduled => "retry_scheduled",
        StoredTranscriptWorkflowStage::ProviderAccepted => "provider_accepted",
        StoredTranscriptWorkflowStage::Cancelled => "cancelled",
        StoredTranscriptWorkflowStage::Blocked if record.may_have_submitted => "ambiguous",
        _ => "failed",
    };
    transaction
        .execute(
            "UPDATE pod0_transcript_attempts SET state=?1,failure_code=?2,failure_detail=?3,
         may_have_submitted=?4,updated_at_ms=?5 WHERE attempt_id=?6 AND state!='committed'",
            params![
                state,
                record.failure_code,
                record.failure_detail,
                i64::from(record.may_have_submitted),
                record.updated_at_ms,
                attempt_id.into_bytes().as_slice()
            ],
        )
        .map_err(|error| StorageError::sqlite("record transcript attempt failure", error))?;
    Ok(())
}
