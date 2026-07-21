use pod0_domain::StateRevision;

use super::ensure_replacement::{next_revision, replacement_record};
use super::model::{
    ModelChapterDesiredPlan, ModelChapterEnsureInput, ModelChapterEnsureOutcome,
    ModelChapterWorkflowRecord, ModelChapterWorkflowState,
};
use super::persist::persist_workflow;
use super::read::read_workflow;
use super::support::{
    validate_blocked_plan, validate_current_model_selection, validate_ensure_values,
    validate_preserved_selection, validate_request,
};
use crate::{LibraryStore, StorageError};

impl LibraryStore {
    pub fn ensure_model_chapter_workflow(
        &self,
        input: ModelChapterEnsureInput,
    ) -> Result<ModelChapterEnsureOutcome, StorageError> {
        validate_ensure_values(
            &input.configured_model,
            input.now_ms,
            input.request_deadline_ms,
            input.max_attempts,
        )?;
        self.write(|transaction| {
            let existing = read_workflow(transaction, input.episode_id)?;
            validate_forced_revision(existing.as_ref(), input.force_retry_from_revision)?;
            validate_plan(transaction, &input)?;
            if let Some(record) = existing.as_ref() {
                if should_keep(record, &input) {
                    return Ok(ModelChapterEnsureOutcome::Existing(record.clone()));
                }
                if protects_attempt(record) && !explicit_retry_allowed(record, &input) {
                    return mark_replan_pending(transaction, record, &input);
                }
            }
            let record = replacement_record(existing.as_ref(), &input)?;
            persist_workflow(transaction, &record)?;
            let stored = read_workflow(transaction, input.episode_id)?
                .ok_or(StorageError::ChapterWorkflowNotFound)?;
            Ok(ModelChapterEnsureOutcome::Changed {
                record: stored,
                replaced: existing.map(Box::new),
            })
        })
    }
}

fn validate_plan(
    transaction: &rusqlite::Transaction<'_>,
    input: &ModelChapterEnsureInput,
) -> Result<(), StorageError> {
    match &input.desired_plan {
        ModelChapterDesiredPlan::Ready(request) => {
            if request.configured_model != input.configured_model {
                return Err(StorageError::ChapterWorkflowConflict);
            }
            validate_request(transaction, input.episode_id, request)
        }
        ModelChapterDesiredPlan::PreserveAgentComposed {
            artifact_id,
            selection_revision,
        } => validate_preserved_selection(
            transaction,
            input.episode_id,
            *artifact_id,
            *selection_revision,
        ),
        ModelChapterDesiredPlan::Current {
            artifact_id,
            selection_revision,
        } => validate_current_model_selection(
            transaction,
            input.episode_id,
            *artifact_id,
            *selection_revision,
        ),
        ModelChapterDesiredPlan::Blocked {
            failure_code,
            failure_detail,
        } => validate_blocked_plan(failure_code, failure_detail.as_deref()),
        ModelChapterDesiredPlan::AwaitingTranscript
        | ModelChapterDesiredPlan::AwaitingPublisher => Ok(()),
    }
}

fn validate_forced_revision(
    existing: Option<&ModelChapterWorkflowRecord>,
    forced: Option<StateRevision>,
) -> Result<(), StorageError> {
    match (existing, forced) {
        (_, None) => Ok(()),
        (Some(record), Some(revision)) if revision == record.workflow_revision => Ok(()),
        _ => Err(StorageError::ChapterWorkflowConflict),
    }
}

fn should_keep(record: &ModelChapterWorkflowRecord, input: &ModelChapterEnsureInput) -> bool {
    if input.force_retry_from_revision.is_some() {
        return false;
    }
    match &input.desired_plan {
        ModelChapterDesiredPlan::AwaitingTranscript => {
            record.state == ModelChapterWorkflowState::AwaitingTranscript
                && record.desired_configured_model == input.configured_model
        }
        ModelChapterDesiredPlan::AwaitingPublisher => {
            record.state == ModelChapterWorkflowState::AwaitingPublisher
                && record.desired_configured_model == input.configured_model
        }
        ModelChapterDesiredPlan::PreserveAgentComposed { artifact_id, .. } => {
            record.state == ModelChapterWorkflowState::Preserved
                && record.selected_artifact_id == Some(*artifact_id)
                && record.desired_configured_model == input.configured_model
        }
        ModelChapterDesiredPlan::Current { artifact_id, .. } => {
            record.state == ModelChapterWorkflowState::Succeeded
                && record.selected_artifact_id == Some(*artifact_id)
                && record.desired_configured_model == input.configured_model
        }
        ModelChapterDesiredPlan::Blocked { failure_code, .. } => {
            record.state == ModelChapterWorkflowState::Blocked
                && record.failure_code.as_deref() == Some(failure_code.as_str())
                && record.desired_configured_model == input.configured_model
        }
        ModelChapterDesiredPlan::Ready(request) => {
            record
                .active_request
                .as_ref()
                .is_some_and(|active| active.request_fingerprint == request.request_fingerprint)
                && record.desired_configured_model == input.configured_model
                && matches!(
                    record.state,
                    ModelChapterWorkflowState::Requested
                        | ModelChapterWorkflowState::SubmissionAuthorized
                        | ModelChapterWorkflowState::ProviderAccepted
                        | ModelChapterWorkflowState::Ambiguous
                        | ModelChapterWorkflowState::CompletionObserved
                        | ModelChapterWorkflowState::RetryScheduled
                        | ModelChapterWorkflowState::Blocked
                        | ModelChapterWorkflowState::Failed
                        | ModelChapterWorkflowState::Cancelled
                        | ModelChapterWorkflowState::Succeeded
                )
        }
    }
}

fn protects_attempt(record: &ModelChapterWorkflowRecord) -> bool {
    record.state.protects_active_attempt()
}

fn explicit_retry_allowed(
    record: &ModelChapterWorkflowRecord,
    input: &ModelChapterEnsureInput,
) -> bool {
    input.force_retry_from_revision == Some(record.workflow_revision)
        && matches!(
            record.state,
            ModelChapterWorkflowState::Ambiguous
                | ModelChapterWorkflowState::Blocked
                | ModelChapterWorkflowState::Failed
                | ModelChapterWorkflowState::Cancelled
        )
}

fn mark_replan_pending(
    transaction: &rusqlite::Transaction<'_>,
    existing: &ModelChapterWorkflowRecord,
    input: &ModelChapterEnsureInput,
) -> Result<ModelChapterEnsureOutcome, StorageError> {
    if existing.replan_pending && existing.desired_configured_model == input.configured_model {
        return Ok(ModelChapterEnsureOutcome::Existing(existing.clone()));
    }
    let mut changed = existing.clone();
    changed.desired_configured_model = input.configured_model.clone();
    changed.replan_pending = true;
    changed.workflow_revision = next_revision(existing.workflow_revision)?;
    changed.updated_at_ms = input.now_ms;
    persist_workflow(transaction, &changed)?;
    Ok(ModelChapterEnsureOutcome::Changed {
        record: changed,
        replaced: None,
    })
}
