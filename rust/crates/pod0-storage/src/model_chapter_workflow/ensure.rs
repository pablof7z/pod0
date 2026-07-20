use pod0_domain::StateRevision;

use super::model::{
    ModelChapterDesiredPlan, ModelChapterEnsureInput, ModelChapterEnsureOutcome,
    ModelChapterWorkflowRecord, ModelChapterWorkflowState,
};
use super::persist::persist_workflow;
use super::read::read_workflow;
use super::support::{
    request_id, submission_fence_id, validate_ensure_values, validate_preserved_selection,
    validate_request,
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

fn replacement_record(
    existing: Option<&ModelChapterWorkflowRecord>,
    input: &ModelChapterEnsureInput,
) -> Result<ModelChapterWorkflowRecord, StorageError> {
    let generation = match (&input.desired_plan, existing) {
        (ModelChapterDesiredPlan::Ready(_), Some(record)) => record
            .generation
            .checked_add(1)
            .ok_or(StorageError::ChapterWorkflowConflict)?,
        (ModelChapterDesiredPlan::Ready(_), None) => 1,
        (_, Some(record)) => record.generation,
        (_, None) => 0,
    };
    let workflow_revision = existing.map_or(Ok(StateRevision::new(1)), |record| {
        next_revision(record.workflow_revision)
    })?;
    let (state, active, selected_artifact_id) = match &input.desired_plan {
        ModelChapterDesiredPlan::AwaitingTranscript => {
            (ModelChapterWorkflowState::AwaitingTranscript, None, None)
        }
        ModelChapterDesiredPlan::AwaitingPublisher => {
            (ModelChapterWorkflowState::AwaitingPublisher, None, None)
        }
        ModelChapterDesiredPlan::PreserveAgentComposed { artifact_id, .. } => (
            ModelChapterWorkflowState::Preserved,
            None,
            Some(*artifact_id),
        ),
        ModelChapterDesiredPlan::Ready(request) => (
            ModelChapterWorkflowState::Requested,
            Some((**request).clone()),
            None,
        ),
    };
    let request = active
        .as_ref()
        .map(|value| request_id(input.episode_id, value.request_fingerprint, generation));
    let fence = request.map(|value| {
        submission_fence_id(
            input.episode_id,
            value,
            input.cancellation_id,
            input.issued_revision,
        )
    });
    let same_fingerprint = existing
        .and_then(|record| record.active_request.as_ref())
        .zip(active.as_ref())
        .is_some_and(|(old, new)| old.request_fingerprint == new.request_fingerprint);
    let attempt = if active.is_some() {
        if same_fingerprint {
            existing
                .expect("same fingerprint has existing record")
                .attempt
                .checked_add(1)
                .ok_or(StorageError::ChapterWorkflowConflict)?
        } else {
            1
        }
    } else {
        0
    };
    Ok(ModelChapterWorkflowRecord {
        episode_id: input.episode_id,
        state,
        desired_configured_model: input.configured_model.clone(),
        active_request: active,
        replan_pending: false,
        generation,
        workflow_revision,
        attempt,
        max_attempts: input.max_attempts,
        command_id: input.command_id,
        cancellation_id: input.cancellation_id,
        request_id: request,
        submission_fence_id: fence,
        issued_revision: input.issued_revision,
        deadline_at_ms: request.map(|_| input.request_deadline_ms),
        not_before_ms: None,
        submission_authorized_at_ms: None,
        provider_operation_id: None,
        provider_status: None,
        selected_artifact_id,
        failure_code: None,
        failure_detail: None,
        may_have_submitted: false,
        created_at_ms: existing.map_or(input.now_ms, |record| record.created_at_ms),
        updated_at_ms: input.now_ms,
    })
}

fn next_revision(current: StateRevision) -> Result<StateRevision, StorageError> {
    current
        .value
        .checked_add(1)
        .map(StateRevision::new)
        .ok_or(StorageError::ChapterWorkflowConflict)
}
