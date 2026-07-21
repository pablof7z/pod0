use pod0_application::{
    CoreWakeReason, HostCancellationRequest, HostObservation, HostRequest, HostRequestEnvelope,
};
use pod0_domain::{HostRequestId, UnixTimestampMilliseconds};
use pod0_storage::ModelChapterWorkflowRecord;
use sha2::{Digest as _, Sha256};

use crate::runtime_state::FacadeState;

const MODEL_FINALIZATION_RETRY_MILLISECONDS: i64 = 1_000;

impl FacadeState {
    pub(super) fn schedule_model_retry_wake(
        &mut self,
        record: &ModelChapterWorkflowRecord,
    ) -> bool {
        let (Some(wake_at), Some(submission_fence_id)) =
            (record.not_before_ms, record.submission_fence_id)
        else {
            return false;
        };
        self.schedule_core_wake(
            record,
            wake_at,
            CoreWakeReason::ModelChapterRetry {
                episode_id: record.episode_id,
                generation: record.generation,
                submission_fence_id,
            },
        )
    }

    pub(super) fn schedule_model_finalization_wake(
        &mut self,
        record: &ModelChapterWorkflowRecord,
    ) -> bool {
        let Some(request_id) = record.request_id else {
            return false;
        };
        self.schedule_core_wake(
            record,
            self.now()
                .value
                .saturating_add(MODEL_FINALIZATION_RETRY_MILLISECONDS),
            CoreWakeReason::ModelChapterFinalization { request_id },
        )
    }

    pub(super) fn finish_core_wake(
        &mut self,
        wake_request_id: HostRequestId,
        observation: HostObservation,
    ) -> bool {
        let Some(reason) = self.pending_core_wakes.remove(&wake_request_id) else {
            return false;
        };
        self.host_requests.retire(wake_request_id);
        let reached = matches!(
            observation,
            HostObservation::CoreWakeReached { reason: observed } if observed == reason
        );
        match reason {
            CoreWakeReason::ModelChapterRetry {
                episode_id,
                generation,
                submission_fence_id,
            } => {
                let record = self
                    .store
                    .as_ref()
                    .and_then(|store| store.model_chapter_workflow(episode_id).ok())
                    .flatten();
                let Some(record) = record.filter(|record| {
                    record.generation == generation
                        && record.submission_fence_id == Some(submission_fence_id)
                }) else {
                    return true;
                };
                if reached
                    && record
                        .not_before_ms
                        .is_none_or(|value| value <= self.now().value)
                {
                    self.queue_model_chapter_request(&record);
                } else {
                    self.schedule_model_retry_wake(&record);
                }
                true
            }
            CoreWakeReason::ModelChapterFinalization { request_id } => {
                if reached && self.resume_staged_model_completion(request_id) {
                    return true;
                }
                let record = self.pending_model_record(request_id).filter(|record| {
                    record.state == pod0_storage::ModelChapterWorkflowState::CompletionObserved
                });
                if let Some(record) = record {
                    self.schedule_model_finalization_wake(&record);
                }
                true
            }
            CoreWakeReason::Unsupported { .. } => true,
        }
    }

    pub(super) fn withdraw_core_wakes_for_model(&mut self, record: &ModelChapterWorkflowRecord) {
        let wake_ids = self
            .pending_core_wakes
            .iter()
            .filter_map(|(request_id, reason)| {
                reason_matches_record(*reason, record).then_some(*request_id)
            })
            .collect::<Vec<_>>();
        for request_id in wake_ids {
            self.pending_core_wakes.remove(&request_id);
            let was_queued = self
                .host_queue
                .iter()
                .any(|request| request.request_id == request_id);
            self.host_queue
                .retain(|request| request.request_id != request_id);
            if self.host_requests.cancel_request(request_id) && !was_queued {
                self.host_cancellations.push_back(HostCancellationRequest {
                    request_id,
                    cancellation_id: record.cancellation_id,
                });
            }
            self.host_requests.retire(request_id);
        }
    }

    pub(super) fn pending_model_record(
        &self,
        request_id: HostRequestId,
    ) -> Option<ModelChapterWorkflowRecord> {
        self.store
            .as_ref()
            .and_then(|store| store.active_model_chapter_workflows(u16::MAX).ok())
            .and_then(|records| {
                records
                    .into_iter()
                    .find(|record| record.request_id == Some(request_id))
            })
    }

    fn schedule_core_wake(
        &mut self,
        record: &ModelChapterWorkflowRecord,
        wake_at_ms: i64,
        reason: CoreWakeReason,
    ) -> bool {
        if wake_at_ms < 0
            || self
                .pending_core_wakes
                .values()
                .any(|value| *value == reason)
        {
            return false;
        }
        let request = HostRequestEnvelope {
            request_id: wake_request_id(reason, wake_at_ms),
            command_id: record.command_id,
            cancellation_id: record.cancellation_id,
            issued_revision: record.issued_revision,
            deadline_at: None,
            request: HostRequest::ScheduleCoreWake {
                wake_at: UnixTimestampMilliseconds::new(wake_at_ms),
                reason,
            },
        };
        if !self.host_requests.register(request.clone()) {
            return false;
        }
        self.pending_core_wakes.insert(request.request_id, reason);
        self.host_queue.push_back(request);
        true
    }
}

fn reason_matches_record(reason: CoreWakeReason, record: &ModelChapterWorkflowRecord) -> bool {
    match reason {
        CoreWakeReason::ModelChapterRetry { episode_id, .. } => episode_id == record.episode_id,
        CoreWakeReason::ModelChapterFinalization { request_id } => {
            Some(request_id) == record.request_id
        }
        CoreWakeReason::Unsupported { .. } => false,
    }
}

fn wake_request_id(reason: CoreWakeReason, wake_at_ms: i64) -> HostRequestId {
    let mut hash = Sha256::new();
    hash.update(b"pod0-core-wake-v1\0");
    hash.update(wake_at_ms.to_be_bytes());
    match reason {
        CoreWakeReason::ModelChapterRetry {
            episode_id,
            generation,
            submission_fence_id,
        } => {
            hash.update([1]);
            hash.update(episode_id.into_bytes());
            hash.update(generation.to_be_bytes());
            hash.update(submission_fence_id.into_bytes());
        }
        CoreWakeReason::ModelChapterFinalization { request_id } => {
            hash.update([2]);
            hash.update(request_id.into_bytes());
        }
        CoreWakeReason::Unsupported { wire_code } => {
            hash.update([255]);
            hash.update(wire_code.to_be_bytes());
        }
    }
    let digest = hash.finalize();
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    HostRequestId::from_bytes(bytes)
}
