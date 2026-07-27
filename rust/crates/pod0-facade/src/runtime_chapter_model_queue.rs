use pod0_application::{
    ChapterModelFailureEvidence, HostCancellationRequest, HostRequest, HostRequestEnvelope,
    MAX_ACTIVE_MODEL_CHAPTER_REQUESTS,
};
use pod0_domain::{HostRequestId, UnixTimestampMilliseconds};
use pod0_storage::{
    ModelChapterFailureDisposition, ModelChapterSubmissionClaim, ModelChapterSubmissionClaimInput,
    ModelChapterWorkflowRecord, ModelChapterWorkflowState, StorageError,
};

use crate::runtime_chapter_model_mapping::execution_request;
use crate::runtime_state::FacadeState;

impl FacadeState {
    pub(super) fn rehydrate_model_chapter_workflows(&mut self) -> Result<(), StorageError> {
        let Some(store) = self.store.clone() else {
            return Ok(());
        };
        let _ = store.recover_model_chapter_workflows(u16::MAX, self.now().value)?;
        let records = store.active_model_chapter_workflows(u16::MAX)?;
        let staged = records
            .iter()
            .filter(|record| record.state == ModelChapterWorkflowState::CompletionObserved)
            .filter_map(|record| record.request_id)
            .collect::<Vec<_>>();
        for record in records {
            self.revision = pod0_domain::StateRevision::new(
                self.revision.value.max(record.workflow_revision.value),
            );
            if matches!(
                record.state,
                ModelChapterWorkflowState::Requested
                    | ModelChapterWorkflowState::RetryScheduled
                    | ModelChapterWorkflowState::ProviderAccepted
            ) {
                self.queue_model_chapter_request(&record);
            }
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

    pub(super) fn admit_model_chapter_request(&mut self) -> Result<(), StorageError> {
        if self.pending_model_chapters.len() >= usize::from(MAX_ACTIVE_MODEL_CHAPTER_REQUESTS) {
            return Ok(());
        }
        let records = self
            .store
            .as_ref()
            .map(|store| {
                store.dispatchable_model_chapter_workflows(MAX_ACTIVE_MODEL_CHAPTER_REQUESTS)
            })
            .transpose()?
            .unwrap_or_default();
        for record in records {
            if matches!(
                record.state,
                ModelChapterWorkflowState::Requested
                    | ModelChapterWorkflowState::RetryScheduled
                    | ModelChapterWorkflowState::ProviderAccepted
            ) {
                self.queue_model_chapter_request(&record);
            }
            if self.pending_model_chapters.len() >= usize::from(MAX_ACTIVE_MODEL_CHAPTER_REQUESTS) {
                break;
            }
        }
        Ok(())
    }

    pub(super) fn prepare_model_chapter_host_request(&mut self) -> bool {
        let _ = self.admit_model_chapter_request();
        let Some((request_id, episode_id)) = self
            .pending_model_chapters
            .iter()
            .find(|(request_id, _)| {
                !self
                    .host_queue
                    .iter()
                    .any(|item| item.request_id == **request_id)
                    && !self.host_requests.is_chapter_model_request(**request_id)
            })
            .map(|(request, episode)| (*request, *episode))
        else {
            return false;
        };
        let Some(store) = self.store.clone() else {
            return false;
        };
        let Ok(Some(record)) = store.model_chapter_workflow(episode_id) else {
            self.pending_model_chapters.remove(&request_id);
            return false;
        };
        if record.request_id != Some(request_id) {
            self.pending_model_chapters.remove(&request_id);
            return false;
        }
        if record.state == ModelChapterWorkflowState::RetryScheduled
            && record
                .not_before_ms
                .is_some_and(|not_before| not_before > self.now().value)
        {
            return self.schedule_model_retry_wake(&record);
        }
        let request = match record.state {
            ModelChapterWorkflowState::Requested | ModelChapterWorkflowState::RetryScheduled => {
                let claim =
                    store.claim_model_chapter_submission(ModelChapterSubmissionClaimInput {
                        episode_id,
                        request_id,
                        generation: record.generation,
                        cancellation_id: record.cancellation_id,
                        issued_revision: record.issued_revision,
                        now_ms: self.now().value,
                    });
                match claim {
                    Ok(ModelChapterSubmissionClaim::Authorized(claimed)) => {
                        execute_host_request(&claimed)
                    }
                    Ok(ModelChapterSubmissionClaim::AlreadyClaimed(claimed)) => {
                        let recovery = recovery_host_request(&claimed);
                        if recovery.is_none() {
                            self.abandon_undeliverable_model_claim(&claimed);
                        }
                        recovery
                    }
                    Err(_) => None,
                }
            }
            ModelChapterWorkflowState::ProviderAccepted => recovery_host_request(&record),
            _ => None,
        };
        let Some(request) = request else {
            return false;
        };
        if !self.host_requests.register(request.clone())
            && !self.host_requests.matches_outstanding(&request)
        {
            if let Ok(Some(claimed)) = store.model_chapter_workflow(episode_id) {
                self.abandon_undeliverable_model_claim(&claimed);
            }
            return false;
        }
        self.host_queue.push_back(request);
        true
    }

    pub(super) fn queue_model_chapter_request(&mut self, record: &ModelChapterWorkflowRecord) {
        let Some(request_id) = record.request_id else {
            return;
        };
        if self.pending_model_chapters.len() < usize::from(MAX_ACTIVE_MODEL_CHAPTER_REQUESTS)
            || self.pending_model_chapters.contains_key(&request_id)
        {
            self.pending_model_chapters
                .insert(request_id, record.episode_id);
        }
    }

    pub(super) fn withdraw_model_chapter_request(&mut self, record: &ModelChapterWorkflowRecord) {
        self.withdraw_core_wakes_for_model(record);
        let Some(request_id) = record.request_id else {
            return;
        };
        let was_queued = self
            .host_queue
            .iter()
            .any(|request| request.request_id == request_id);
        self.host_queue
            .retain(|request| request.request_id != request_id);
        self.pending_model_chapters.remove(&request_id);
        self.pending_model_observations.remove(&request_id);
        if self.host_requests.cancel_request(request_id) && !was_queued {
            self.host_cancellations.push_back(HostCancellationRequest {
                request_id,
                cancellation_id: record.cancellation_id,
            });
        }
        self.host_requests.retire(request_id);
    }

    pub(super) fn retire_model_chapter_request(&mut self, request_id: HostRequestId) {
        if let Some(record) = self.pending_model_record(request_id) {
            self.withdraw_core_wakes_for_model(&record);
        }
        self.pending_model_chapters.remove(&request_id);
        self.pending_model_observations.remove(&request_id);
        self.host_requests.retire(request_id);
        let _ = self.admit_model_chapter_request();
    }

    fn abandon_undeliverable_model_claim(&mut self, record: &ModelChapterWorkflowRecord) {
        if record.state == ModelChapterWorkflowState::SubmissionAuthorized {
            let _ = self.commit_model_chapter_failure(
                record,
                ChapterModelFailureEvidence::Transport {
                    submission_authorized: true,
                },
                ModelChapterFailureDisposition::Ambiguous,
                Some("authorized model request could not be delivered safely".into()),
            );
        }
    }
}

fn execute_host_request(record: &ModelChapterWorkflowRecord) -> Option<HostRequestEnvelope> {
    let active = record.active_request.as_ref()?;
    Some(HostRequestEnvelope {
        request_id: record.request_id?,
        command_id: record.command_id,
        cancellation_id: record.cancellation_id,
        issued_revision: record.issued_revision,
        deadline_at: record.deadline_at_ms.map(UnixTimestampMilliseconds::new),
        request: HostRequest::ExecuteChapterModel {
            episode_id: record.episode_id,
            generation: record.generation,
            submission_fence_id: record.submission_fence_id?,
            execution: execution_request(active)?,
        },
    })
}

fn recovery_host_request(record: &ModelChapterWorkflowRecord) -> Option<HostRequestEnvelope> {
    let active = record.active_request.as_ref()?;
    Some(HostRequestEnvelope {
        request_id: record.request_id?,
        command_id: record.command_id,
        cancellation_id: record.cancellation_id,
        issued_revision: record.issued_revision,
        deadline_at: None,
        request: HostRequest::RecoverChapterModelOperation {
            episode_id: record.episode_id,
            generation: record.generation,
            submission_fence_id: record.submission_fence_id?,
            provider: active.provider.clone(),
            model: active.model.clone(),
            provider_operation_id: record.provider_operation_id.clone()?,
            provider_status: record.provider_status.clone(),
            maximum_completion_bytes: active.maximum_completion_bytes,
        },
    })
}
