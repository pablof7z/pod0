use pod0_application::{CoreWakeReason, DurableLifecycleEffectRequest};
use pod0_domain::HostRequestId;
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
            record.command_id,
            record.cancellation_id,
            record.issued_revision,
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
            record.command_id,
            record.cancellation_id,
            record.issued_revision,
            self.now()
                .value
                .saturating_add(MODEL_FINALIZATION_RETRY_MILLISECONDS),
            CoreWakeReason::ModelChapterFinalization { request_id },
        )
    }

    pub(super) fn apply_core_wake_reaction(
        &mut self,
        reason: CoreWakeReason,
        reached: bool,
    ) -> bool {
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
                if !(reached
                    && record
                        .not_before_ms
                        .is_none_or(|value| value <= self.now().value))
                {
                    return self.schedule_model_retry_wake(&record);
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
            CoreWakeReason::TranscriptProviderRecovery {
                episode_id,
                attempt_id,
                submission_fence_id,
            } => self.finish_transcript_wake(
                episode_id,
                attempt_id,
                submission_fence_id,
                false,
                reached,
            ),
            CoreWakeReason::TranscriptRetry {
                episode_id,
                attempt_id,
                submission_fence_id,
            } => self.finish_transcript_wake(
                episode_id,
                attempt_id,
                submission_fence_id,
                true,
                reached,
            ),
            CoreWakeReason::TranscriptFinalization { request_id } => {
                let record = self.pending_transcript_record(request_id);
                if reached
                    && record
                        .as_ref()
                        .is_some_and(|record| self.finalize_transcript_completion(record))
                {
                    return true;
                }
                if let Some(record) = record.filter(|record| {
                    record.stage == pod0_storage::StoredTranscriptWorkflowStage::CompletionObserved
                }) {
                    self.schedule_transcript_finalization_wake(&record);
                }
                true
            }
            CoreWakeReason::FeedDiscoveryNotificationRetry { .. }
            | CoreWakeReason::FeedFetchRetry { .. } => true,
            CoreWakeReason::Unsupported { .. } => true,
        }
    }

    pub(super) fn withdraw_core_wakes_for_model(&mut self, record: &ModelChapterWorkflowRecord) {
        self.cancel_lifecycle_wakes(record.command_id, record.cancellation_id);
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

    pub(super) fn schedule_core_wake(
        &mut self,
        command_id: pod0_domain::CommandId,
        cancellation_id: pod0_domain::CancellationId,
        issued_revision: pod0_domain::StateRevision,
        wake_at_ms: i64,
        reason: CoreWakeReason,
    ) -> bool {
        if wake_at_ms < 0 {
            return false;
        }
        let request = DurableLifecycleEffectRequest {
            request_id: wake_request_id(reason, wake_at_ms),
            command_id,
            cancellation_id,
            issued_revision,
            wake_at: pod0_domain::UnixTimestampMilliseconds::new(wake_at_ms),
            reason,
            attempt: 1,
        };
        self.store
            .as_ref()
            .is_some_and(|store| store.authorize_lifecycle_wake(request, self.now()).is_ok())
    }

    pub(super) fn cancel_lifecycle_wakes(
        &mut self,
        command_id: pod0_domain::CommandId,
        cancellation_id: pod0_domain::CancellationId,
    ) {
        let mut hash = Sha256::new();
        hash.update(b"pod0-lifecycle-wake-cancel-v1\0");
        hash.update(command_id.into_bytes());
        hash.update(cancellation_id.into_bytes());
        let digest: [u8; 32] = hash.finalize().into();
        let cancellation_command =
            pod0_domain::CommandId::from_bytes(digest[..16].try_into().expect("digest prefix"));
        let fingerprint = pod0_domain::ContentDigest::from_bytes(digest);
        if let Some(store) = &self.store {
            let _ = store.cancel_durable_lifecycle_wakes(
                cancellation_command,
                fingerprint,
                cancellation_id,
                self.now(),
            );
        }
    }
}

include!("runtime_core_wake_identity.rs");
