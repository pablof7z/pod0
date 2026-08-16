use pod0_application::{
    ActivityFailureCode, ChapterModelFailureEvidence, HostObservation, HostObservationReceipt,
    HostObservationRejection, LeasedHostObservationEnvelope, OperationStage,
    classify_chapter_model_failure,
};
use pod0_storage::{
    ModelChapterCompletionInput, ModelChapterFailureInput, ModelChapterObservationAction,
    ModelChapterObservationCommitInput, ModelChapterProviderAcceptedInput,
    ModelChapterWorkflowRecord, ModelChapterWorkflowState,
};

use crate::runtime_chapter_model_mapping::{failure_wire, host_failure_evidence};
use crate::runtime_chapter_model_receipts::{
    failure_disposition, generic_host_failure, persisted, rejected, retain, storage_receipt,
};
use crate::runtime_state::{FacadeState, failure};

impl FacadeState {
    pub(super) fn record_leased_model_chapter_observation(
        &mut self,
        leased: LeasedHostObservationEnvelope,
    ) -> (bool, HostObservationReceipt) {
        let request_id = leased.observation.request_id;
        let acceptance = self.host_requests.validate_observation(&leased.observation);
        if !matches!(
            acceptance,
            pod0_application::ObservationAcceptance::Accepted
                | pod0_application::ObservationAcceptance::UnknownRequest
                | pod0_application::ObservationAcceptance::Duplicate
        ) {
            return (
                false,
                crate::runtime_observation_mapping::rejected(request_id, acceptance),
            );
        }
        let Some(store) = self.store.clone() else {
            return (false, retain(request_id));
        };
        let record = match model_record(&store, &leased) {
            Ok(record) => record,
            Err(receipt) => return (false, receipt),
        };
        let action = match self.model_action(&record, &leased.observation.observation) {
            Some(action) => action,
            None => {
                return (
                    false,
                    rejected(request_id, HostObservationRejection::MismatchedPayload),
                );
            }
        };
        let committed =
            match store.commit_model_chapter_observation(ModelChapterObservationCommitInput {
                lease: leased.lease,
                observation: leased.observation,
                action: action.clone(),
                committed_at: self.now(),
            }) {
                Ok(value) => value,
                Err(error) => return (false, storage_receipt(request_id, error)),
            };
        if committed.replayed {
            return (
                false,
                rejected(request_id, HostObservationRejection::Duplicate),
            );
        }
        if committed.terminal_effect {
            self.retire_model_chapter_request(request_id);
        }
        self.project_model_observation(&record, &committed.workflow, &action);
        self.advance_revision();
        (true, persisted(request_id, committed.terminal_effect))
    }

    fn model_action(
        &self,
        record: &ModelChapterWorkflowRecord,
        observation: &HostObservation,
    ) -> Option<ModelChapterObservationAction> {
        let now_ms = self.now().value;
        match observation {
            HostObservation::ChapterModelProviderAccepted {
                episode_id,
                generation,
                submission_fence_id,
                update,
            } => Some(ModelChapterObservationAction::ProviderAccepted(
                ModelChapterProviderAcceptedInput {
                    episode_id: *episode_id,
                    request_id: record.request_id?,
                    generation: *generation,
                    submission_fence_id: *submission_fence_id,
                    provider_operation_id: update.provider_operation_id.clone(),
                    provider_status: update.provider_status.clone(),
                    observed_at_ms: now_ms,
                },
            )),
            HostObservation::ChapterModelCompleted {
                episode_id,
                generation,
                submission_fence_id,
                completion,
            } => Some(ModelChapterObservationAction::Completion(
                ModelChapterCompletionInput {
                    episode_id: *episode_id,
                    request_id: record.request_id?,
                    generation: *generation,
                    submission_fence_id: *submission_fence_id,
                    completion: completion.completion.clone(),
                    provider: completion.provider.clone(),
                    model: completion.model.clone(),
                    prompt_tokens: completion.prompt_tokens,
                    completion_tokens: completion.completion_tokens,
                    cached_tokens: completion.cached_tokens,
                    reasoning_tokens: completion.reasoning_tokens,
                    cost_microusd: completion.cost_microusd,
                    provider_operation_id: completion.provider_operation_id.clone(),
                    provider_status: completion.provider_status.clone(),
                    generated_at_ms: completion
                        .provider_generated_at
                        .map_or(now_ms, |value| value.value),
                    observed_at_ms: now_ms,
                },
            )),
            HostObservation::ChapterModelFailed {
                code,
                safe_detail,
                retry_after_milliseconds,
                ..
            } => Some(self.model_failure_action(
                record,
                host_failure_evidence(*code),
                safe_detail.clone(),
                *retry_after_milliseconds,
                false,
            )),
            HostObservation::Failed { code, safe_detail } => Some(self.model_failure_action(
                record,
                host_failure_evidence(generic_host_failure(*code)),
                safe_detail.clone(),
                None,
                false,
            )),
            HostObservation::Cancelled => Some(self.model_failure_action(
                record,
                ChapterModelFailureEvidence::Cancelled {
                    submission_authorized: true,
                },
                None,
                None,
                true,
            )),
            _ => None,
        }
    }

    fn model_failure_action(
        &self,
        record: &ModelChapterWorkflowRecord,
        evidence: ChapterModelFailureEvidence,
        safe_detail: Option<String>,
        retry_after_milliseconds: Option<u64>,
        cancelled: bool,
    ) -> ModelChapterObservationAction {
        let classification = classify_chapter_model_failure(evidence);
        let disposition = failure_disposition(
            record,
            classification,
            self.revision,
            self.now().value,
            retry_after_milliseconds.map(|value| i64::try_from(value).unwrap_or(i64::MAX)),
        );
        ModelChapterObservationAction::Failure {
            input: ModelChapterFailureInput {
                episode_id: record.episode_id,
                request_id: record.request_id.expect("active model request"),
                generation: record.generation,
                submission_fence_id: record.submission_fence_id.expect("submitted model request"),
                failure_code: failure_wire(classification.code).to_owned(),
                failure_detail: safe_detail,
                may_have_submitted: classification.may_have_submitted,
                disposition,
                observed_at_ms: self.now().value,
            },
            outcome: if cancelled {
                pod0_application::EffectOutcome::Cancelled
            } else {
                pod0_application::EffectOutcome::Failed {
                    code: activity_failure(classification.code),
                }
            },
        }
    }

    fn project_model_observation(
        &mut self,
        original: &ModelChapterWorkflowRecord,
        updated: &ModelChapterWorkflowRecord,
        action: &ModelChapterObservationAction,
    ) {
        match action {
            ModelChapterObservationAction::ProviderAccepted(_) => {}
            ModelChapterObservationAction::Completion(_) => {
                let finalized = self.resume_staged_model_completion(
                    original.request_id.expect("active model request"),
                );
                if !finalized {
                    self.schedule_model_finalization_wake(updated);
                }
            }
            ModelChapterObservationAction::Failure { input, .. } => {
                if matches!(
                    updated.state,
                    ModelChapterWorkflowState::Requested
                        | ModelChapterWorkflowState::RetryScheduled
                ) {
                    self.finish(original.command_id, OperationStage::Running, None, None);
                } else {
                    self.finish(
                        original.command_id,
                        OperationStage::Failed,
                        Some(failure(core_failure_wire(&input.failure_code))),
                        None,
                    );
                }
            }
        }
    }
}

fn model_record(
    store: &pod0_storage::LibraryStore,
    leased: &LeasedHostObservationEnvelope,
) -> Result<ModelChapterWorkflowRecord, HostObservationReceipt> {
    match store.model_chapter_workflow_for_effect_intent(leased.lease.intent_id) {
        Ok(Some(record)) if record.request_id == Some(leased.observation.request_id) => Ok(record),
        Ok(_) => Err(rejected(
            leased.observation.request_id,
            HostObservationRejection::StaleWorkflow,
        )),
        Err(_) => Err(retain(leased.observation.request_id)),
    }
}

fn activity_failure(
    code: pod0_application::ModelChapterWorkflowFailureCode,
) -> ActivityFailureCode {
    use pod0_application::ModelChapterWorkflowFailureCode as Code;
    match code {
        Code::Offline => ActivityFailureCode::Offline,
        Code::TimedOut => ActivityFailureCode::TimedOut,
        Code::MissingCredential => ActivityFailureCode::PermissionDenied,
        Code::ResponseTooLarge => ActivityFailureCode::ResponseTooLarge,
        Code::ProviderUnavailable | Code::Transport => ActivityFailureCode::ProviderUnavailable,
        _ => ActivityFailureCode::InvalidResponse,
    }
}

fn core_failure_wire(code: &str) -> pod0_application::CoreFailureCode {
    match code {
        "stale_transcript" | "stale_publisher_base" | "selection_changed" => {
            pod0_application::CoreFailureCode::RevisionConflict
        }
        "offline" | "timed_out" | "transport" | "provider_unavailable" => {
            pod0_application::CoreFailureCode::HostUnavailable
        }
        _ => pod0_application::CoreFailureCode::HostRejected,
    }
}
