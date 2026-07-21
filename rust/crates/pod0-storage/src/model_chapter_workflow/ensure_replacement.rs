use pod0_domain::StateRevision;

use super::model::{
    ModelChapterDesiredPlan, ModelChapterEnsureInput, ModelChapterWorkflowRecord,
    ModelChapterWorkflowState,
};
use super::support::{request_id, submission_fence_id};
use crate::StorageError;

pub(super) fn replacement_record(
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
    let (state, active, selected_artifact_id, failure_code, failure_detail) =
        match &input.desired_plan {
            ModelChapterDesiredPlan::AwaitingTranscript => (
                ModelChapterWorkflowState::AwaitingTranscript,
                None,
                None,
                None,
                None,
            ),
            ModelChapterDesiredPlan::AwaitingPublisher => (
                ModelChapterWorkflowState::AwaitingPublisher,
                None,
                None,
                None,
                None,
            ),
            ModelChapterDesiredPlan::PreserveAgentComposed { artifact_id, .. } => (
                ModelChapterWorkflowState::Preserved,
                None,
                Some(*artifact_id),
                None,
                None,
            ),
            ModelChapterDesiredPlan::Current { artifact_id, .. } => (
                ModelChapterWorkflowState::Succeeded,
                None,
                Some(*artifact_id),
                None,
                None,
            ),
            ModelChapterDesiredPlan::Blocked {
                failure_code,
                failure_detail,
            } => (
                ModelChapterWorkflowState::Blocked,
                None,
                None,
                Some(failure_code.clone()),
                failure_detail.clone(),
            ),
            ModelChapterDesiredPlan::Ready(request) => (
                ModelChapterWorkflowState::Requested,
                Some((**request).clone()),
                None,
                None,
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
        failure_code,
        failure_detail,
        may_have_submitted: false,
        created_at_ms: existing.map_or(input.now_ms, |record| record.created_at_ms),
        updated_at_ms: input.now_ms,
    })
}

pub(super) fn next_revision(current: StateRevision) -> Result<StateRevision, StorageError> {
    current
        .value
        .checked_add(1)
        .map(StateRevision::new)
        .ok_or(StorageError::ChapterWorkflowConflict)
}
