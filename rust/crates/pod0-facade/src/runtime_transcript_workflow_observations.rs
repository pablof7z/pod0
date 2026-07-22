use pod0_application::{
    HostObservation, HostObservationEnvelope, HostObservationReceipt, HostObservationRejection,
    TRANSCRIPT_RETRY_BASE_MILLISECONDS, TranscriptCapabilityObservation, TranscriptFailureEvidence,
    TranscriptRetryDisposition, classify_transcript_failure,
};
use pod0_storage::{
    TranscriptCompletionInput, TranscriptProviderAcceptedInput, TranscriptProviderPendingInput,
    TranscriptWorkflowFailureInput, TranscriptWorkflowRecord,
};

use crate::runtime_chapter_model_receipts::{persisted, rejected, retain};
use crate::runtime_state::FacadeState;
use crate::runtime_transcript_workflow_receipts::{
    failure_disposition, failure_wire, storage_receipt,
};

impl FacadeState {
    pub(super) fn retry_pending_transcript_observation(
        &mut self,
        request_id: pod0_domain::HostRequestId,
        observation: &HostObservationEnvelope,
    ) -> Option<(bool, HostObservationReceipt)> {
        let pending = self.pending_transcript_observations.get(&request_id)?;
        if pending != observation {
            return Some((false, retain(request_id)));
        }
        let Some(record) = self.pending_transcript_record(request_id) else {
            return Some((false, retain(request_id)));
        };
        let receipt = self.persist_transcript_observation(record, observation.clone());
        let changed = matches!(receipt, HostObservationReceipt::Persisted { .. });
        if !matches!(receipt, HostObservationReceipt::RetainAndRetry { .. }) {
            self.pending_transcript_observations.remove(&request_id);
        }
        Some((changed, receipt))
    }

    pub(super) fn persist_transcript_observation(
        &mut self,
        record: TranscriptWorkflowRecord,
        envelope: HostObservationEnvelope,
    ) -> HostObservationReceipt {
        let request_id = envelope.request_id;
        if record.request_id != Some(request_id) {
            return rejected(request_id, HostObservationRejection::StaleWorkflow);
        }
        let HostObservation::TranscriptCapabilityObserved { observation } = envelope.observation
        else {
            return rejected(request_id, HostObservationRejection::MismatchedPayload);
        };
        match observation {
            TranscriptCapabilityObservation::ProviderAccepted {
                external_operation_id,
                provider_status,
            } => self.persist_transcript_provider_accepted(
                record,
                request_id,
                external_operation_id,
                provider_status,
            ),
            TranscriptCapabilityObservation::ProviderPending {
                provider_status,
                retry_after_milliseconds,
            } => self.persist_transcript_provider_pending(
                record,
                request_id,
                provider_status,
                retry_after_milliseconds,
            ),
            TranscriptCapabilityObservation::Completed {
                external_operation_id,
                provider_status,
                artifact,
            } => {
                let Some(store) = self.store.clone() else {
                    return retain(request_id);
                };
                match store.stage_transcript_workflow_completion(TranscriptCompletionInput {
                    episode_id: record.episode_id,
                    request_id,
                    attempt_id: record.attempt_id,
                    submission_fence_id: record.submission_fence_id,
                    external_operation_id,
                    provider_status,
                    artifact,
                    observed_at_ms: self.now().value,
                }) {
                    Ok(staged) => {
                        self.advance_revision();
                        if !self.finalize_transcript_completion(&staged) {
                            self.schedule_transcript_finalization_wake(&staged);
                        }
                        persisted(request_id, true)
                    }
                    Err(error) => storage_receipt(request_id, error),
                }
            }
            TranscriptCapabilityObservation::Failed {
                evidence,
                safe_detail,
                retry_after_milliseconds,
            } => self.persist_transcript_failure(
                record,
                request_id,
                evidence,
                safe_detail,
                retry_after_milliseconds,
            ),
            TranscriptCapabilityObservation::Cancelled => {
                let evidence = TranscriptFailureEvidence::Cancelled {
                    submission_authorized: record.submission_authorized_at_ms.is_some(),
                    provider_accepted: record.external_operation_id.is_some(),
                };
                self.persist_transcript_failure(record, request_id, evidence, None, None)
            }
        }
    }

    fn persist_transcript_provider_accepted(
        &mut self,
        record: TranscriptWorkflowRecord,
        request_id: pod0_domain::HostRequestId,
        external_operation_id: String,
        provider_status: Option<String>,
    ) -> HostObservationReceipt {
        let (Some(attempt_id), Some(submission_fence_id), Some(store)) = (
            record.attempt_id,
            record.submission_fence_id,
            self.store.clone(),
        ) else {
            return rejected(request_id, HostObservationRejection::StaleWorkflow);
        };
        let now = self.now().value;
        let accepted = store.record_transcript_provider_accepted(TranscriptProviderAcceptedInput {
            episode_id: record.episode_id,
            request_id,
            attempt_id,
            submission_fence_id,
            external_operation_id,
            provider_status: provider_status.clone(),
            observed_at_ms: now,
        });
        let Ok(_) = accepted else {
            return storage_receipt(request_id, accepted.expect_err("checked failure"));
        };
        let pending = store.record_transcript_provider_pending(TranscriptProviderPendingInput {
            episode_id: record.episode_id,
            request_id,
            attempt_id,
            submission_fence_id,
            provider_status,
            not_before_ms: now.saturating_add(TRANSCRIPT_RETRY_BASE_MILLISECONDS),
            observed_at_ms: now,
        });
        match pending {
            Ok(updated) => {
                self.advance_revision();
                self.retire_transcript_request(request_id);
                self.queue_transcript_request(&updated);
                self.schedule_transcript_wake(&updated);
                persisted(request_id, false)
            }
            Err(error) => storage_receipt(request_id, error),
        }
    }

    fn persist_transcript_provider_pending(
        &mut self,
        record: TranscriptWorkflowRecord,
        request_id: pod0_domain::HostRequestId,
        provider_status: Option<String>,
        retry_after_milliseconds: Option<u64>,
    ) -> HostObservationReceipt {
        let (Some(attempt_id), Some(submission_fence_id), Some(store)) = (
            record.attempt_id,
            record.submission_fence_id,
            self.store.clone(),
        ) else {
            return rejected(request_id, HostObservationRejection::StaleWorkflow);
        };
        let now = self.now().value;
        let delay = retry_after_milliseconds
            .and_then(|value| i64::try_from(value).ok())
            .unwrap_or(TRANSCRIPT_RETRY_BASE_MILLISECONDS)
            .max(TRANSCRIPT_RETRY_BASE_MILLISECONDS);
        match store.record_transcript_provider_pending(TranscriptProviderPendingInput {
            episode_id: record.episode_id,
            request_id,
            attempt_id,
            submission_fence_id,
            provider_status,
            not_before_ms: now.saturating_add(delay),
            observed_at_ms: now,
        }) {
            Ok(updated) => {
                self.advance_revision();
                self.retire_transcript_request(request_id);
                self.queue_transcript_request(&updated);
                self.schedule_transcript_wake(&updated);
                persisted(request_id, false)
            }
            Err(error) => storage_receipt(request_id, error),
        }
    }

    fn persist_transcript_failure(
        &mut self,
        record: TranscriptWorkflowRecord,
        request_id: pod0_domain::HostRequestId,
        evidence: TranscriptFailureEvidence,
        safe_detail: Option<String>,
        retry_after_milliseconds: Option<u64>,
    ) -> HostObservationReceipt {
        let classification = classify_transcript_failure(evidence);
        let disposition = failure_disposition(
            &record,
            classification,
            self.revision,
            self.now().value,
            retry_after_milliseconds,
        );
        let Some(store) = self.store.clone() else {
            return retain(request_id);
        };
        match store.fail_transcript_workflow(TranscriptWorkflowFailureInput {
            episode_id: record.episode_id,
            request_id,
            attempt_id: record.attempt_id,
            submission_fence_id: record.submission_fence_id,
            failure_code: failure_wire(classification.code).to_owned(),
            failure_detail: safe_detail,
            retryable: !matches!(classification.retry, TranscriptRetryDisposition::Never),
            may_have_submitted: classification.may_have_submitted,
            disposition,
            observed_at_ms: self.now().value,
        }) {
            Ok(updated) => {
                self.advance_revision();
                self.retire_transcript_request(request_id);
                if matches!(
                    updated.stage,
                    pod0_storage::StoredTranscriptWorkflowStage::RetryScheduled
                        | pod0_storage::StoredTranscriptWorkflowStage::ProviderAccepted
                ) {
                    self.queue_transcript_request(&updated);
                    self.schedule_transcript_wake(&updated);
                }
                persisted(request_id, true)
            }
            Err(error) => storage_receipt(request_id, error),
        }
    }
}
