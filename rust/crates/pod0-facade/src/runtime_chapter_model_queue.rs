use pod0_application::MAX_ACTIVE_MODEL_CHAPTER_REQUESTS;
use pod0_domain::HostRequestId;
use pod0_storage::{
    ModelChapterSubmissionClaim, ModelChapterSubmissionClaimInput, ModelChapterWorkflowRecord,
    ModelChapterWorkflowState, StorageError,
};

use crate::runtime_state::FacadeState;

impl FacadeState {
    pub(super) fn rehydrate_model_chapter_workflows(&mut self) -> Result<(), StorageError> {
        let Some(store) = self.store.clone() else {
            return Ok(());
        };
        let _ = store.recover_model_chapter_workflows(u16::MAX, self.now().value)?;
        let mut records = store.active_model_chapter_workflows(u16::MAX)?;
        for record in records
            .iter_mut()
            .filter(|record| record.state == ModelChapterWorkflowState::ProviderAccepted)
        {
            *record = store
                .authorize_model_chapter_provider_recovery(record.episode_id, self.now().value)?;
        }
        let staged = records
            .iter()
            .filter(|record| record.state == ModelChapterWorkflowState::CompletionObserved)
            .filter_map(|record| record.request_id)
            .collect::<Vec<_>>();
        for record in records {
            self.revision = pod0_domain::StateRevision::new(
                self.revision.value.max(record.workflow_revision.value),
            );
        }
        for request_id in staged {
            if !self.resume_staged_model_completion(request_id)
                && let Some(record) = self.pending_model_record(request_id)
            {
                self.schedule_model_finalization_wake(&record);
            }
        }
        Ok(())
    }

    pub(super) fn prepare_model_chapter_host_request(&mut self) -> bool {
        let Some(store) = self.store.clone() else {
            return false;
        };
        let Ok(records) = store.dispatchable_model_chapter_workflows(
            MAX_ACTIVE_MODEL_CHAPTER_REQUESTS,
        ) else {
            return false;
        };
        let Some(record) = records.into_iter().next() else { return false };
        let Some(request_id) = record.request_id else { return false };
        if record.state == ModelChapterWorkflowState::RetryScheduled
            && record
                .not_before_ms
                .is_some_and(|not_before| not_before > self.now().value)
        {
            return self.schedule_model_retry_wake(&record);
        }
        match record.state {
            ModelChapterWorkflowState::Requested | ModelChapterWorkflowState::RetryScheduled => {
                match store.claim_model_chapter_submission(ModelChapterSubmissionClaimInput {
                    episode_id: record.episode_id,
                    request_id,
                    generation: record.generation,
                    cancellation_id: record.cancellation_id,
                    issued_revision: record.issued_revision,
                    now_ms: self.now().value,
                }) {
                    Ok(ModelChapterSubmissionClaim::Authorized(_))
                    | Ok(ModelChapterSubmissionClaim::AlreadyClaimed(_)) => true,
                    Err(_) => false,
                }
            }
            ModelChapterWorkflowState::ProviderAccepted => true,
            _ => false,
        }
    }

    pub(super) fn withdraw_model_chapter_request(&mut self, record: &ModelChapterWorkflowRecord) {
        self.withdraw_core_wakes_for_model(record);
        let Some(request_id) = record.request_id else {
            return;
        };
        self.host_requests.cancel_request(request_id);
        self.host_requests.retire(request_id);
    }

    pub(super) fn retire_model_chapter_request(&mut self, request_id: HostRequestId) {
        if let Some(record) = self.pending_model_record(request_id) {
            self.withdraw_core_wakes_for_model(&record);
        }
        self.host_requests.retire(request_id);
    }
}
