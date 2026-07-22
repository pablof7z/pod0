use pod0_application::{
    HostCancellationRequest, TranscriptFailureEvidence, classify_transcript_failure,
};
use pod0_domain::HostRequestId;
use pod0_storage::{
    StoredTranscriptWorkflowStage, TranscriptSubmissionClaim, TranscriptSubmissionClaimInput,
    TranscriptWorkflowFailureDisposition, TranscriptWorkflowFailureInput, TranscriptWorkflowRecord,
};

use crate::runtime_state::FacadeState;
use crate::runtime_transcript_workflow_mapping::host_request;
use crate::runtime_transcript_workflow_receipts::failure_wire;

impl FacadeState {
    pub(super) fn rehydrate_transcript_workflows(
        &mut self,
    ) -> Result<(), pod0_storage::StorageError> {
        let Some(store) = self.store.clone() else {
            return Ok(());
        };
        if !store.transcript_workflow_authority()?.is_authoritative() {
            return Ok(());
        }
        let _ = store.recover_transcript_workflows(self.now().value, u16::MAX)?;
        let page = store.transcript_workflow_page(0, u16::MAX)?;
        for record in page.items {
            self.revision = pod0_domain::StateRevision::new(
                self.revision.value.max(record.workflow_revision.value),
            );
            if matches!(
                record.stage,
                StoredTranscriptWorkflowStage::Requested
                    | StoredTranscriptWorkflowStage::PublisherRequested
                    | StoredTranscriptWorkflowStage::RetryScheduled
                    | StoredTranscriptWorkflowStage::ProviderAccepted
            ) {
                self.queue_transcript_request(&record);
            }
            if record.stage == StoredTranscriptWorkflowStage::CompletionObserved
                && !self.finalize_transcript_completion(&record)
            {
                self.schedule_transcript_finalization_wake(&record);
            }
            if record.stage == StoredTranscriptWorkflowStage::EvidenceRequested {
                let _ = self.resume_transcript_evidence(&record);
            }
        }
        Ok(())
    }

    pub(super) fn prepare_transcript_host_request(&mut self) -> bool {
        let Some((request_id, episode_id)) = self
            .pending_transcripts
            .iter()
            .find(|(request_id, _)| {
                !self
                    .host_queue
                    .iter()
                    .any(|item| item.request_id == **request_id)
                    && !self.host_requests.is_transcript_request(**request_id)
            })
            .map(|(request_id, episode_id)| (*request_id, *episode_id))
        else {
            return false;
        };
        let Some(store) = self.store.clone() else {
            return false;
        };
        let Ok(Some(record)) = store.transcript_workflow(episode_id) else {
            self.pending_transcripts.remove(&request_id);
            return false;
        };
        if record.request_id != Some(request_id) {
            self.pending_transcripts.remove(&request_id);
            return false;
        }
        if record.stage == StoredTranscriptWorkflowStage::RetryScheduled
            && record
                .not_before_ms
                .is_some_and(|value| value > self.now().value)
        {
            return self.schedule_transcript_wake(&record);
        }
        if record.stage == StoredTranscriptWorkflowStage::ProviderAccepted
            && record
                .not_before_ms
                .is_some_and(|value| value > self.now().value)
        {
            return self.schedule_transcript_wake(&record);
        }
        let record = match record.stage {
            StoredTranscriptWorkflowStage::PublisherRequested => record,
            StoredTranscriptWorkflowStage::Requested
            | StoredTranscriptWorkflowStage::RetryScheduled => {
                let (Some(attempt_id), Some(submission_fence_id)) =
                    (record.attempt_id, record.submission_fence_id)
                else {
                    self.pending_transcripts.remove(&request_id);
                    return false;
                };
                match store.claim_transcript_submission(TranscriptSubmissionClaimInput {
                    episode_id,
                    request_id,
                    attempt_id,
                    submission_fence_id,
                    cancellation_id: record.cancellation_id,
                    issued_revision: record.issued_revision,
                    now_ms: self.now().value,
                }) {
                    Ok(TranscriptSubmissionClaim::Authorized(claimed)) => claimed,
                    Ok(TranscriptSubmissionClaim::AlreadyClaimed(claimed)) => {
                        if claimed.external_operation_id.is_none() {
                            self.abandon_undeliverable_transcript_claim(&claimed);
                            return false;
                        }
                        claimed
                    }
                    Err(_) => return false,
                }
            }
            StoredTranscriptWorkflowStage::ProviderAccepted => record,
            _ => return false,
        };
        let Some(podcast_id) = self
            .listening
            .episodes
            .iter()
            .find(|episode| episode.episode_id == episode_id)
            .map(|episode| episode.podcast_id)
        else {
            return false;
        };
        let Some(request) = host_request(&record, podcast_id) else {
            self.abandon_undeliverable_transcript_claim(&record);
            return false;
        };
        if !self.host_requests.register(request.clone())
            && !self.host_requests.matches_outstanding(&request)
        {
            self.abandon_undeliverable_transcript_claim(&record);
            return false;
        }
        self.host_queue.push_back(request);
        true
    }

    pub(super) fn queue_transcript_request(&mut self, record: &TranscriptWorkflowRecord) {
        if let Some(request_id) = record.request_id {
            self.pending_transcripts
                .insert(request_id, record.episode_id);
        }
    }

    pub(super) fn withdraw_transcript_request(&mut self, record: &TranscriptWorkflowRecord) {
        self.withdraw_core_wakes_for_transcript(record);
        let Some(request_id) = record.request_id else {
            return;
        };
        let was_queued = self
            .host_queue
            .iter()
            .any(|item| item.request_id == request_id);
        self.host_queue.retain(|item| item.request_id != request_id);
        self.pending_transcripts.remove(&request_id);
        self.pending_transcript_observations.remove(&request_id);
        if self.host_requests.cancel_request(request_id) && !was_queued {
            self.host_cancellations.push_back(HostCancellationRequest {
                request_id,
                cancellation_id: record.cancellation_id,
            });
        }
        self.host_requests.retire(request_id);
    }

    pub(super) fn retire_transcript_request(&mut self, request_id: HostRequestId) {
        self.pending_transcripts.remove(&request_id);
        self.pending_transcript_observations.remove(&request_id);
        self.host_requests.retire(request_id);
    }

    pub(super) fn pending_transcript_record(
        &self,
        request_id: HostRequestId,
    ) -> Option<TranscriptWorkflowRecord> {
        let episode_id = self.pending_transcripts.get(&request_id)?;
        self.store
            .as_ref()?
            .transcript_workflow(*episode_id)
            .ok()?
            .filter(|record| record.request_id == Some(request_id))
    }

    fn abandon_undeliverable_transcript_claim(&mut self, record: &TranscriptWorkflowRecord) {
        let classification = classify_transcript_failure(TranscriptFailureEvidence::Transport {
            submission_authorized: true,
            provider_accepted: record.stage == StoredTranscriptWorkflowStage::ProviderAccepted,
        });
        let Some(request_id) = record.request_id else {
            return;
        };
        let _ = self.store.as_ref().and_then(|store| {
            store
                .fail_transcript_workflow(TranscriptWorkflowFailureInput {
                    episode_id: record.episode_id,
                    request_id,
                    attempt_id: record.attempt_id,
                    submission_fence_id: record.submission_fence_id,
                    failure_code: failure_wire(classification.code).to_owned(),
                    failure_detail: Some(
                        "authorized transcript request could not be delivered safely".into(),
                    ),
                    retryable: false,
                    may_have_submitted: true,
                    disposition: TranscriptWorkflowFailureDisposition::Ambiguous,
                    observed_at_ms: self.now().value,
                })
                .ok()
        });
        self.retire_transcript_request(request_id);
    }
}
