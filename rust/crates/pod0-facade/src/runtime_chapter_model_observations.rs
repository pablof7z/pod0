use pod0_application::{
    ChapterModelFailureEvidence, HostObservation, HostObservationEnvelope, HostObservationReceipt,
    HostObservationRejection, classify_chapter_model_failure,
};
use pod0_storage::{
    ModelChapterCompletionInput, ModelChapterFailureDisposition, ModelChapterFailureInput,
    ModelChapterProviderAcceptedInput, ModelChapterWorkflowRecord, ModelChapterWorkflowState,
};

use crate::runtime_chapter_model_mapping::{
    failure_wire, host_failure_evidence, request_is_current,
};
use crate::runtime_chapter_model_receipts::{
    completion_observation_replays, core_failure, failure_disposition, generic_host_failure,
    persisted, rejected, retain, storage_receipt,
};
use crate::runtime_state::{FacadeState, failure};

impl FacadeState {
    pub(super) fn replayed_model_completion_receipt(
        &self,
        envelope: &HostObservationEnvelope,
    ) -> Option<HostObservationReceipt> {
        let HostObservation::ChapterModelCompleted {
            episode_id,
            generation,
            submission_fence_id,
            completion,
        } = &envelope.observation
        else {
            return None;
        };
        let Some(store) = self.store.as_ref() else {
            return Some(retain(envelope.request_id));
        };
        match store.model_chapter_completion(envelope.request_id) {
            Ok(Some(existing))
                if completion_observation_replays(
                    &existing,
                    envelope,
                    *episode_id,
                    *generation,
                    *submission_fence_id,
                    completion,
                ) =>
            {
                Some(persisted(envelope.request_id, true))
            }
            Ok(Some(_)) => Some(rejected(
                envelope.request_id,
                HostObservationRejection::StaleWorkflow,
            )),
            Ok(None) => None,
            Err(_) => Some(retain(envelope.request_id)),
        }
    }

    pub(super) fn persist_model_observation(
        &mut self,
        record: ModelChapterWorkflowRecord,
        envelope: HostObservationEnvelope,
    ) -> HostObservationReceipt {
        let request_id = envelope.request_id;
        if !request_is_current(&record, request_id) {
            return rejected(request_id, HostObservationRejection::StaleWorkflow);
        }
        match envelope.observation {
            HostObservation::ChapterModelProviderAccepted {
                episode_id,
                generation,
                submission_fence_id,
                update,
            } => {
                let kernel_observed_at_ms = self.now().value;
                let Some(store) = self.store.as_ref() else {
                    return retain(request_id);
                };
                match store.record_model_chapter_provider_accepted(
                    ModelChapterProviderAcceptedInput {
                        episode_id,
                        request_id,
                        generation,
                        submission_fence_id,
                        provider_operation_id: update.provider_operation_id,
                        provider_status: update.provider_status,
                        observed_at_ms: kernel_observed_at_ms,
                    },
                ) {
                    Ok(_) => {
                        self.advance_revision();
                        persisted(request_id, false)
                    }
                    Err(error) => storage_receipt(request_id, error),
                }
            }
            HostObservation::ChapterModelCompleted {
                episode_id,
                generation,
                submission_fence_id,
                completion,
            } => {
                let kernel_completed_at_ms = self.now().value;
                let Some(store) = self.store.as_ref() else {
                    return retain(request_id);
                };
                let staged = store.stage_model_chapter_completion(ModelChapterCompletionInput {
                    episode_id,
                    request_id,
                    generation,
                    submission_fence_id,
                    completion: completion.completion,
                    provider: completion.provider,
                    model: completion.model,
                    prompt_tokens: completion.prompt_tokens,
                    completion_tokens: completion.completion_tokens,
                    cached_tokens: completion.cached_tokens,
                    reasoning_tokens: completion.reasoning_tokens,
                    cost_microusd: completion.cost_microusd,
                    provider_operation_id: completion.provider_operation_id,
                    provider_status: completion.provider_status,
                    generated_at_ms: completion
                        .provider_generated_at
                        .map_or(kernel_completed_at_ms, |value| value.value),
                    observed_at_ms: kernel_completed_at_ms,
                });
                match staged {
                    Ok(_) => {
                        self.advance_revision();
                        let finalized = self.resume_staged_model_completion(request_id);
                        self.retire_model_chapter_request(request_id);
                        if !finalized && let Some(record) = self.pending_model_record(request_id) {
                            self.schedule_model_finalization_wake(&record);
                        }
                        persisted(request_id, true)
                    }
                    Err(error) => storage_receipt(request_id, error),
                }
            }
            HostObservation::ChapterModelFailed {
                code,
                safe_detail,
                retry_after_milliseconds,
                ..
            } => self.persist_model_failure(
                record,
                host_failure_evidence(code),
                safe_detail,
                retry_after_milliseconds,
            ),
            HostObservation::Failed { code, safe_detail } => self.persist_model_failure(
                record,
                host_failure_evidence(generic_host_failure(code)),
                safe_detail,
                None,
            ),
            HostObservation::Cancelled => self.persist_model_failure(
                record,
                ChapterModelFailureEvidence::Cancelled {
                    submission_authorized: true,
                },
                None,
                None,
            ),
            _ => self.persist_model_failure(
                record,
                ChapterModelFailureEvidence::InvalidResponse,
                None,
                None,
            ),
        }
    }

    pub(super) fn persist_oversized_model_observation(
        &mut self,
        record: ModelChapterWorkflowRecord,
    ) -> HostObservationReceipt {
        self.persist_model_failure(
            record,
            ChapterModelFailureEvidence::ResponseTooLarge,
            None,
            None,
        )
    }

    fn persist_model_failure(
        &mut self,
        record: ModelChapterWorkflowRecord,
        evidence: ChapterModelFailureEvidence,
        safe_detail: Option<String>,
        retry_after_milliseconds: Option<u64>,
    ) -> HostObservationReceipt {
        let classification = classify_chapter_model_failure(evidence);
        let disposition = failure_disposition(
            &record,
            classification,
            self.revision,
            self.now().value,
            retry_after_milliseconds.map(|value| i64::try_from(value).unwrap_or(i64::MAX)),
        );
        let request_id = record
            .request_id
            .expect("active model workflow has request identity");
        if self.commit_model_chapter_failure(&record, evidence, disposition, safe_detail) {
            persisted(request_id, true)
        } else {
            retain(request_id)
        }
    }

    pub(super) fn commit_model_chapter_failure(
        &mut self,
        record: &ModelChapterWorkflowRecord,
        evidence: ChapterModelFailureEvidence,
        disposition: ModelChapterFailureDisposition,
        safe_detail: Option<String>,
    ) -> bool {
        let Some(store) = self.store.clone() else {
            return false;
        };
        let classification = classify_chapter_model_failure(evidence);
        let request_id = record
            .request_id
            .expect("active model workflow has request identity");
        let Some(fence) = record.submission_fence_id else {
            return false;
        };
        match store.fail_model_chapter_workflow(ModelChapterFailureInput {
            episode_id: record.episode_id,
            request_id,
            generation: record.generation,
            submission_fence_id: fence,
            failure_code: failure_wire(classification.code).to_owned(),
            failure_detail: safe_detail,
            may_have_submitted: classification.may_have_submitted,
            disposition,
            observed_at_ms: self.now().value,
        }) {
            Ok(next) => {
                self.advance_revision();
                self.retire_model_chapter_request(request_id);
                if matches!(
                    next.state,
                    ModelChapterWorkflowState::Requested
                        | ModelChapterWorkflowState::RetryScheduled
                ) {
                    self.queue_model_chapter_request(&next);
                    self.finish(
                        record.command_id,
                        pod0_application::OperationStage::Running,
                        None,
                        None,
                    );
                } else {
                    self.finish(
                        record.command_id,
                        pod0_application::OperationStage::Failed,
                        Some(failure(core_failure(classification.code))),
                        None,
                    );
                }
                true
            }
            Err(_) => false,
        }
    }
}
