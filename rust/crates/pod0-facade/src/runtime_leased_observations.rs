use pod0_application::{
    DurableAgentApprovalHostObservation, DurableAgentModelHostObservation,
    DurableRecallHostObservation, HostObservationReceipt, HostObservationRejection,
    LeasedHostObservationEnvelope, ObservationAcceptance,
};
use pod0_storage::{
    AgentApprovalObservationCommitInput, AgentModelObservationCommitInput,
    EvidenceObservationCommitInput, StoredTranscriptWorkflowStage, TranscriptWorkflowRecord,
};

use crate::runtime_chapter_model_receipts::{persisted, retain};
use crate::runtime_evidence_state::{EvidenceIndexCompletion, PendingEvidenceIndex};
use crate::runtime_observation_mapping::rejected;
use crate::runtime_state::FacadeState;

impl FacadeState {
    pub(super) fn record_leased_agent_model_observation(
        &mut self,
        leased: LeasedHostObservationEnvelope,
        durable: DurableAgentModelHostObservation,
    ) -> (bool, HostObservationReceipt) {
        let request_id = durable.request_id;
        let Some(store) = self.agent_store.clone() else {
            return (false, retain(request_id));
        };
        let committed = match store.commit_model_observation(AgentModelObservationCommitInput {
            lease: leased.lease,
            observation: durable,
            committed_at: self.now(),
        }) {
            Ok(value) => value,
            Err(pod0_storage::StorageError::AgentTurnConflict) => {
                return (
                    false,
                    rejected_payload(request_id, HostObservationRejection::StaleWorkflow),
                );
            }
            Err(_) => return (false, retain(request_id)),
        };
        if committed.replayed {
            return (
                false,
                rejected_payload(request_id, HostObservationRejection::Duplicate),
            );
        }
        self.resume_agent_internal_commands();
        self.host_requests.retire(request_id);
        self.advance_revision();
        (true, persisted(request_id, true))
    }

    pub(super) fn record_leased_agent_approval_observation(
        &mut self,
        leased: LeasedHostObservationEnvelope,
        durable: DurableAgentApprovalHostObservation,
    ) -> (bool, HostObservationReceipt) {
        let request_id = durable.request_id;
        let Some(store) = self.agent_store.clone() else {
            return (false, retain(request_id));
        };
        let committed =
            match store.commit_approval_observation(AgentApprovalObservationCommitInput {
                lease: leased.lease,
                observation: durable,
                committed_at: self.now(),
            }) {
                Ok(value) => value,
                Err(pod0_storage::StorageError::AgentTurnConflict) => {
                    return (
                        false,
                        rejected_payload(request_id, HostObservationRejection::StaleWorkflow),
                    );
                }
                Err(_) => return (false, retain(request_id)),
            };
        if committed.replayed {
            return (
                false,
                rejected_payload(request_id, HostObservationRejection::Duplicate),
            );
        }
        self.resume_agent_internal_commands();
        self.host_requests.retire(request_id);
        self.advance_revision();
        (true, persisted(request_id, true))
    }

    pub(super) fn record_leased_recall_observation(
        &mut self,
        leased: LeasedHostObservationEnvelope,
        durable: DurableRecallHostObservation,
    ) -> (bool, HostObservationReceipt) {
        let request_id = leased.observation.request_id;
        let acceptance = self.host_requests.validate_observation(&leased.observation);
        if acceptance != ObservationAcceptance::Accepted {
            return (false, rejected(request_id, acceptance));
        }
        let Some(store) = self.store.clone() else {
            return (false, retain(request_id));
        };
        let pending = self
            .pending_evidence_indexes
            .remove(&request_id)
            .or_else(|| self.reconstruct_pending_evidence(&durable));
        let Some(pending) = pending else {
            return (false, retain(request_id));
        };
        let committed = match store.commit_evidence_observation(EvidenceObservationCommitInput {
            lease: leased.lease,
            observation: durable.clone(),
            committed_at: self.now(),
        }) {
            Ok(value) => value,
            Err(_) => return (false, retain(request_id)),
        };
        if committed.replayed {
            return (
                false,
                rejected_payload(request_id, HostObservationRejection::Duplicate),
            );
        }
        if self.host_requests.accept_observation(&leased.observation)
            != ObservationAcceptance::Accepted
        {
            return (false, retain(request_id));
        }
        self.finish_evidence_index_observation(pending, leased.observation.observation);
        self.advance_revision();
        (true, persisted(request_id, true))
    }

    fn reconstruct_pending_evidence(
        &self,
        observation: &DurableRecallHostObservation,
    ) -> Option<PendingEvidenceIndex> {
        let workflow = self
            .store
            .as_ref()?
            .transcript_workflow(observation.episode_id)
            .ok()??;
        let artifact = self
            .evidence_store
            .as_ref()?
            .selected_artifact(observation.episode_id)
            .ok()??;
        (artifact.generation_id == observation.generation_id).then_some(PendingEvidenceIndex {
            command_id: workflow.command_id,
            cancellation_id: workflow.cancellation_id,
            episode_id: observation.episode_id,
            generation_id: observation.generation_id,
            expected_span_count: u32::try_from(artifact.spans.len()).ok()?,
            requested_span_ids: observation
                .embeddings
                .iter()
                .map(|embedding| embedding.span_id)
                .collect(),
            completion: EvidenceIndexCompletion::TranscriptWorkflow {
                workflow_id: workflow.request.workflow_id,
                input_version: workflow.evidence_input_version?,
            },
        })
    }

    pub(super) fn apply_committed_transcript_observation(
        &mut self,
        record: &TranscriptWorkflowRecord,
    ) {
        match record.stage {
            StoredTranscriptWorkflowStage::ProviderAccepted
            | StoredTranscriptWorkflowStage::RetryScheduled => {
                self.queue_transcript_request(record);
                self.schedule_transcript_wake(record);
            }
            StoredTranscriptWorkflowStage::CompletionObserved => {
                if !self.finalize_transcript_completion(record) {
                    self.schedule_transcript_finalization_wake(record);
                }
            }
            _ => {}
        }
    }
}

pub(crate) fn rejected_payload(
    request_id: pod0_domain::HostRequestId,
    reason: HostObservationRejection,
) -> HostObservationReceipt {
    HostObservationReceipt::Rejected { request_id, reason }
}
