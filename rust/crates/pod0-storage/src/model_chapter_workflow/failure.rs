use pod0_domain::StateRevision;

use super::inputs::{ModelChapterFailureDisposition, ModelChapterFailureInput};
use super::model::{ModelChapterWorkflowRecord, ModelChapterWorkflowState};
use super::persist::persist_workflow;
use super::read::{read_completion, read_workflow};
use super::support::{request_id, submission_fence_id};
use crate::{LibraryStore, StorageError};

impl LibraryStore {
    pub fn fail_model_chapter_workflow(
        &self,
        input: ModelChapterFailureInput,
    ) -> Result<ModelChapterWorkflowRecord, StorageError> {
        validate_failure(&input)?;
        self.write(|transaction| {
            let mut record = exact_failure_record(transaction, &input)?;
            if !matches!(
                record.state,
                ModelChapterWorkflowState::Requested
                    | ModelChapterWorkflowState::RetryScheduled
                    | ModelChapterWorkflowState::SubmissionAuthorized
                    | ModelChapterWorkflowState::ProviderAccepted
                    | ModelChapterWorkflowState::CompletionObserved
            ) {
                return Err(StorageError::ChapterWorkflowConflict);
            }
            if matches!(&input.disposition, ModelChapterFailureDisposition::Replan)
                && record.may_have_submitted
                && (record.state != ModelChapterWorkflowState::CompletionObserved
                    || read_completion(transaction, input.request_id)?.is_none())
            {
                return Err(StorageError::ChapterWorkflowConflict);
            }
            record.workflow_revision = next_revision(record.workflow_revision)?;
            record.failure_code = Some(input.failure_code);
            record.failure_detail = input.failure_detail;
            record.may_have_submitted |= input.may_have_submitted;
            record.updated_at_ms = input.observed_at_ms;
            apply_disposition(&mut record, input.disposition)?;
            persist_workflow(transaction, &record)?;
            read_workflow(transaction, record.episode_id)?
                .ok_or(StorageError::ChapterWorkflowNotFound)
        })
    }
}

fn validate_failure(input: &ModelChapterFailureInput) -> Result<(), StorageError> {
    if input.observed_at_ms < 0
        || input.failure_code.is_empty()
        || input.failure_code.len() > 256
        || input
            .failure_detail
            .as_ref()
            .is_some_and(|value| value.len() > 16_384)
    {
        return Err(StorageError::ChapterWorkflowConflict);
    }
    Ok(())
}

fn exact_failure_record(
    transaction: &rusqlite::Transaction<'_>,
    input: &ModelChapterFailureInput,
) -> Result<ModelChapterWorkflowRecord, StorageError> {
    let record = read_workflow(transaction, input.episode_id)?
        .ok_or(StorageError::ChapterWorkflowNotFound)?;
    if record.request_id != Some(input.request_id)
        || record.generation != input.generation
        || record.submission_fence_id != Some(input.submission_fence_id)
    {
        return Err(StorageError::ChapterWorkflowConflict);
    }
    Ok(record)
}

fn apply_disposition(
    record: &mut ModelChapterWorkflowRecord,
    disposition: ModelChapterFailureDisposition,
) -> Result<(), StorageError> {
    match disposition {
        ModelChapterFailureDisposition::Retry {
            not_before_ms,
            deadline_at_ms,
            issued_revision,
            evidence_permits_resubmission,
        } => retry(
            record,
            not_before_ms,
            deadline_at_ms,
            issued_revision,
            evidence_permits_resubmission,
        ),
        ModelChapterFailureDisposition::Replan => {
            record.state = ModelChapterWorkflowState::Blocked;
            record.replan_pending = true;
            record.deadline_at_ms = None;
            record.not_before_ms = None;
            Ok(())
        }
        ModelChapterFailureDisposition::Block => {
            terminal(record, ModelChapterWorkflowState::Blocked)
        }
        ModelChapterFailureDisposition::Fail => terminal(record, ModelChapterWorkflowState::Failed),
        ModelChapterFailureDisposition::Ambiguous => {
            record.state = ModelChapterWorkflowState::Ambiguous;
            record.may_have_submitted = true;
            record.deadline_at_ms = None;
            record.not_before_ms = None;
            Ok(())
        }
        ModelChapterFailureDisposition::Cancel => {
            if record.may_have_submitted {
                return Err(StorageError::ChapterWorkflowConflict);
            }
            terminal(record, ModelChapterWorkflowState::Cancelled)
        }
    }
}

fn retry(
    record: &mut ModelChapterWorkflowRecord,
    not_before_ms: i64,
    deadline_at_ms: i64,
    issued_revision: StateRevision,
    evidence_permits_resubmission: bool,
) -> Result<(), StorageError> {
    if not_before_ms < record.updated_at_ms
        || deadline_at_ms < not_before_ms
        || record.attempt >= record.max_attempts
        || (record.may_have_submitted && !evidence_permits_resubmission)
    {
        return Err(StorageError::ChapterWorkflowConflict);
    }
    let active = record
        .active_request
        .as_ref()
        .ok_or(StorageError::ChapterWorkflowConflict)?;
    record.generation = record
        .generation
        .checked_add(1)
        .ok_or(StorageError::ChapterWorkflowConflict)?;
    record.attempt = record
        .attempt
        .checked_add(1)
        .ok_or(StorageError::ChapterWorkflowConflict)?;
    let request = request_id(
        record.episode_id,
        active.request_fingerprint,
        record.generation,
    );
    record.request_id = Some(request);
    record.submission_fence_id = Some(submission_fence_id(
        record.episode_id,
        request,
        record.cancellation_id,
        issued_revision,
    ));
    record.issued_revision = issued_revision;
    record.state = ModelChapterWorkflowState::RetryScheduled;
    record.deadline_at_ms = Some(deadline_at_ms);
    record.not_before_ms = Some(not_before_ms);
    record.submission_authorized_at_ms = None;
    record.provider_operation_id = None;
    record.provider_status = None;
    record.selected_artifact_id = None;
    record.may_have_submitted = false;
    Ok(())
}

fn terminal(
    record: &mut ModelChapterWorkflowRecord,
    state: ModelChapterWorkflowState,
) -> Result<(), StorageError> {
    record.state = state;
    record.deadline_at_ms = None;
    record.not_before_ms = None;
    Ok(())
}

fn next_revision(current: StateRevision) -> Result<StateRevision, StorageError> {
    current
        .value
        .checked_add(1)
        .map(StateRevision::new)
        .ok_or(StorageError::ChapterWorkflowConflict)
}
