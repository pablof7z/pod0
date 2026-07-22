use pod0_application::CoreWakeReason;
use pod0_domain::{EpisodeId, TranscriptAttemptId, TranscriptSubmissionFenceId};
use pod0_storage::{StoredTranscriptWorkflowStage, TranscriptWorkflowRecord};

use crate::runtime_state::FacadeState;

impl FacadeState {
    pub(super) fn schedule_transcript_finalization_wake(
        &mut self,
        record: &TranscriptWorkflowRecord,
    ) -> bool {
        let Some(request_id) = record.request_id else {
            return false;
        };
        self.schedule_core_wake(
            record.command_id,
            record.cancellation_id,
            record.issued_revision,
            self.now().value.saturating_add(1_000),
            CoreWakeReason::TranscriptFinalization { request_id },
        )
    }

    pub(super) fn schedule_transcript_wake(&mut self, record: &TranscriptWorkflowRecord) -> bool {
        let (Some(wake_at_ms), Some(attempt_id), Some(submission_fence_id)) = (
            record.not_before_ms,
            record.attempt_id,
            record.submission_fence_id,
        ) else {
            return false;
        };
        let reason = match record.stage {
            StoredTranscriptWorkflowStage::ProviderAccepted => {
                CoreWakeReason::TranscriptProviderRecovery {
                    episode_id: record.episode_id,
                    attempt_id,
                    submission_fence_id,
                }
            }
            StoredTranscriptWorkflowStage::RetryScheduled => CoreWakeReason::TranscriptRetry {
                episode_id: record.episode_id,
                attempt_id,
                submission_fence_id,
            },
            _ => return false,
        };
        self.schedule_core_wake(
            record.command_id,
            record.cancellation_id,
            record.issued_revision,
            wake_at_ms,
            reason,
        )
    }

    pub(super) fn finish_transcript_wake(
        &mut self,
        episode_id: EpisodeId,
        attempt_id: TranscriptAttemptId,
        submission_fence_id: TranscriptSubmissionFenceId,
        retry: bool,
        reached: bool,
    ) -> bool {
        let record = self
            .store
            .as_ref()
            .and_then(|store| store.transcript_workflow(episode_id).ok())
            .flatten();
        let Some(record) = record.filter(|record| {
            record.attempt_id == Some(attempt_id)
                && record.submission_fence_id == Some(submission_fence_id)
                && record.stage
                    == if retry {
                        StoredTranscriptWorkflowStage::RetryScheduled
                    } else {
                        StoredTranscriptWorkflowStage::ProviderAccepted
                    }
        }) else {
            return true;
        };
        if reached
            && record
                .not_before_ms
                .is_none_or(|value| value <= self.now().value)
        {
            self.queue_transcript_request(&record);
        } else {
            self.schedule_transcript_wake(&record);
        }
        true
    }

    pub(super) fn withdraw_core_wakes_for_transcript(&mut self, record: &TranscriptWorkflowRecord) {
        let wake_ids = self
            .pending_core_wakes
            .iter()
            .filter_map(|(request_id, reason)| {
                transcript_reason_matches(*reason, record).then_some(*request_id)
            })
            .collect::<Vec<_>>();
        for request_id in wake_ids {
            self.pending_core_wakes.remove(&request_id);
            self.host_queue
                .retain(|request| request.request_id != request_id);
            self.host_requests.retire(request_id);
        }
    }
}

fn transcript_reason_matches(reason: CoreWakeReason, record: &TranscriptWorkflowRecord) -> bool {
    match reason {
        CoreWakeReason::TranscriptProviderRecovery { episode_id, .. }
        | CoreWakeReason::TranscriptRetry { episode_id, .. } => episode_id == record.episode_id,
        CoreWakeReason::TranscriptFinalization { request_id } => {
            Some(request_id) == record.request_id
        }
        _ => false,
    }
}
