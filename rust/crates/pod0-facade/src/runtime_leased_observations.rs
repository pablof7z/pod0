use pod0_application::{
    DurableAgentApprovalHostObservation, DurableAgentModelHostObservation, DurableEffectExecution,
    DurableEvidenceEmbeddingEffectRequest, DurableRecallHostObservation, HostObservationReceipt,
    HostObservationRejection, LeasedHostObservationEnvelope,
};
use pod0_storage::{
    AgentApprovalObservationCommitInput, AgentModelObservationCommitInput,
    EvidenceObservationCommitInput, StoredTranscriptWorkflowStage, TranscriptWorkflowRecord,
};

use crate::runtime_chapter_model_receipts::{persisted, retain};
use crate::runtime_evidence_commands::pending_from_effect;
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
        let Some(store) = self.store.clone() else {
            return (false, retain(request_id));
        };
        let request = store
            .effect_request(leased.lease.intent_id)
            .ok()
            .flatten()
            .and_then(|effect| match effect.execution {
                DurableEffectExecution::EvidenceEmbedding { request } => Some(request),
                _ => None,
            });
        let Some(request) = request else {
            return (false, stale(request_id));
        };
        if !matches_evidence_observation(&request, &durable) {
            return (false, mismatched(request_id));
        };
        let pending = pending_from_effect(request);
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
        self.host_requests.retire(request_id);
        self.finish_evidence_index_observation(pending, leased.observation.observation);
        self.advance_revision();
        (true, persisted(request_id, true))
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

fn matches_evidence_observation(
    request: &DurableEvidenceEmbeddingEffectRequest,
    observation: &DurableRecallHostObservation,
) -> bool {
    request.request_id == observation.request_id
        && request.cancellation_id == observation.cancellation_id
        && request.issued_revision == observation.observed_request_revision
        && request.episode_id == observation.episode_id
        && request.generation_id == observation.generation_id
        && request.spans.len() == observation.embeddings.len()
        && request.spans.iter().all(|span| {
            observation
                .embeddings
                .iter()
                .any(|embedding| embedding.span_id == span.span_id)
        })
}

fn stale(request_id: pod0_domain::HostRequestId) -> HostObservationReceipt {
    rejected_payload(request_id, HostObservationRejection::StaleWorkflow)
}

fn mismatched(request_id: pod0_domain::HostRequestId) -> HostObservationReceipt {
    rejected_payload(request_id, HostObservationRejection::MismatchedPayload)
}

pub(crate) fn rejected_payload(
    request_id: pod0_domain::HostRequestId,
    reason: HostObservationRejection,
) -> HostObservationReceipt {
    HostObservationReceipt::Rejected { request_id, reason }
}
