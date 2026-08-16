use crate::runtime_state::FacadeState;
use pod0_application::{ActivitySubject, HostRequest, HostRequestEnvelope};
use pod0_domain::CommandId;
use pod0_recall_index::RECALL_INDEX_DIMENSIONS;

impl FacadeState {
    pub(super) fn recall_request_for_effect(
        &mut self,
        lease: &pod0_storage::EffectLease,
    ) -> Option<HostRequestEnvelope> {
        if let pod0_application::DurableEffectExecution::AgentRecall { request } =
            &lease.request.execution
        {
            if lease.subject
                != (ActivitySubject::AgentTurn {
                    turn_id: request.turn_id,
                })
                || lease.request.deadline_at != Some(request.deadline_at)
            {
                return None;
            }
            let host = recall_host_request(request)?;
            return Some(HostRequestEnvelope {
                request_id: request.request_id,
                command_id: CommandId::from_bytes(lease.intent_id.into_bytes()),
                cancellation_id: request.cancellation_id,
                issued_revision: request.issued_revision,
                deadline_at: Some(request.deadline_at),
                request: host,
            });
        }
        if let pod0_application::DurableEffectExecution::RecallQuery { request } =
            &lease.request.execution
        {
            if lease.subject != ActivitySubject::Global
                || lease.request.deadline_at != Some(request.deadline_at)
            {
                return None;
            }
            return Some(HostRequestEnvelope {
                request_id: request.request_id,
                command_id: request.command_id,
                cancellation_id: request.cancellation_id,
                issued_revision: request.issued_revision,
                deadline_at: Some(request.deadline_at),
                request: recall_query_host_request(request)?,
            });
        }
        if let pod0_application::DurableEffectExecution::RecallIndexCutover { request } =
            &lease.request.execution
        {
            if lease.subject != ActivitySubject::Global
                || lease.request.deadline_at != Some(request.deadline_at)
            {
                return None;
            }
            return Some(HostRequestEnvelope {
                request_id: request.request_id,
                command_id: request.command_id,
                cancellation_id: request.cancellation_id,
                issued_revision: request.issued_revision,
                deadline_at: Some(request.deadline_at),
                request: HostRequest::RemoveLegacyRecallIndexArtifacts,
            });
        }
        self.evidence_recall_request_for_effect(lease)
    }

    fn evidence_recall_request_for_effect(
        &mut self,
        lease: &pod0_storage::EffectLease,
    ) -> Option<HostRequestEnvelope> {
        let pod0_application::DurableEffectExecution::EvidenceEmbedding { request } =
            &lease.request.execution
        else {
            return None;
        };
        if lease.subject
            != (ActivitySubject::Episode {
                episode_id: request.episode_id,
            })
            || lease.request.episode_id != Some(request.episode_id)
            || lease.request.deadline_at != Some(request.deadline_at)
        {
            return None;
        }
        Some(HostRequestEnvelope {
            request_id: request.request_id,
            command_id: request.command_id,
            cancellation_id: request.cancellation_id,
            issued_revision: request.issued_revision,
            deadline_at: Some(request.deadline_at),
            request: HostRequest::EmbedRecallSpans {
                episode_id: request.episode_id,
                generation_id: request.generation_id,
                provider: request.provider,
                model: request.model.clone(),
                spans: request.spans.clone(),
                maximum_dimensions: u16::try_from(RECALL_INDEX_DIMENSIONS).ok()?,
            },
        })
    }
}

fn recall_host_request(
    request: &pod0_application::DurableAgentRecallEffectRequest,
) -> Option<HostRequest> {
    match &request.phase {
        pod0_application::AgentRecallEffectPhase::EmbedQuery => {
            Some(HostRequest::EmbedRecallQuery {
                query_id: request.query.query_id,
                provider: request.embedding_provider,
                model: request.embedding_model.clone(),
                text: request.query.text.clone(),
                maximum_dimensions: u16::try_from(RECALL_INDEX_DIMENSIONS).ok()?,
            })
        }
        pod0_application::AgentRecallEffectPhase::Rerank { candidates, .. } => {
            let (provider, model) = request.reranker.clone()?;
            Some(HostRequest::RerankRecallCandidates {
                query_id: request.query.query_id,
                provider,
                model,
                query: request.query.text.clone(),
                candidates: candidates.clone(),
            })
        }
    }
}

fn recall_query_host_request(
    request: &pod0_application::DurableRecallQueryEffectRequest,
) -> Option<HostRequest> {
    match &request.phase {
        pod0_application::AgentRecallEffectPhase::EmbedQuery => {
            Some(HostRequest::EmbedRecallQuery {
                query_id: request.query.query_id,
                provider: request.embedding_provider,
                model: request.embedding_model.clone(),
                text: request.query.text.clone(),
                maximum_dimensions: u16::try_from(RECALL_INDEX_DIMENSIONS).ok()?,
            })
        }
        pod0_application::AgentRecallEffectPhase::Rerank { candidates, .. } => {
            let (provider, model) = request.reranker.clone()?;
            Some(HostRequest::RerankRecallCandidates {
                query_id: request.query.query_id,
                provider,
                model,
                query: request.query.text.clone(),
                candidates: candidates.clone(),
            })
        }
    }
}
