use pod0_application::{
    AgentRecallEffectPhase, AgentRecallHostOutcome, DurableAgentRecallEffectRequest,
    DurableAgentRecallHostObservation, DurableEffectExecution, HostObservationReceipt,
    HostObservationRejection, LeasedHostObservationEnvelope, RecallEvidenceProjection,
    RecallRerankDocument,
};
use pod0_recall_index::{RecallIndexCandidate, RecallIndexError};
use pod0_storage::{AgentRecallObservationCommitInput, AgentRecallResolution};

use crate::runtime_chapter_model_receipts::{persisted, retain};
use crate::runtime_leased_observations::rejected_payload;
use crate::runtime_recall_rerank::validate_rerank;
use crate::runtime_recall_resolution::{CandidateResolutionError, resolve_candidates};
use crate::runtime_state::FacadeState;

impl FacadeState {
    pub(crate) fn record_leased_agent_recall_observation(
        &mut self,
        leased: LeasedHostObservationEnvelope,
        observation: DurableAgentRecallHostObservation,
    ) -> (bool, HostObservationReceipt) {
        let request_id = observation.request_id;
        let Some(store) = self.agent_store.clone() else {
            return (false, retain(request_id));
        };
        let request = self
            .store
            .as_ref()
            .and_then(|value| value.effect_request(leased.lease.intent_id).ok().flatten())
            .and_then(|value| match value.execution {
                DurableEffectExecution::AgentRecall { request } => Some(request),
                _ => None,
            });
        let Some(request) = request else {
            return (false, stale(request_id));
        };
        let resolution = match self.resolve_agent_recall(
            &request,
            &observation.outcome,
            observation.observed_at,
        ) {
            Ok(value) => value,
            Err(_) => return (false, mismatched(request_id)),
        };
        let committed = store.commit_recall_observation(AgentRecallObservationCommitInput {
            lease: leased.lease,
            observation,
            resolution,
            committed_at: self.now(),
        });
        let committed = match committed {
            Ok(value) => value,
            Err(pod0_storage::StorageError::AgentTurnConflict) => {
                return (false, stale(request_id));
            }
            Err(_) => return (false, retain(request_id)),
        };
        if committed.replayed {
            return (false, duplicate(request_id));
        }
        self.host_requests.retire(request_id);
        self.advance_revision();
        (true, persisted(request_id, !committed.continued))
    }

    fn resolve_agent_recall(
        &self,
        request: &DurableAgentRecallEffectRequest,
        outcome: &AgentRecallHostOutcome,
        observed_at: pod0_domain::UnixTimestampMilliseconds,
    ) -> Result<AgentRecallResolution, ()> {
        match (&request.phase, outcome) {
            (
                AgentRecallEffectPhase::EmbedQuery,
                AgentRecallHostOutcome::QueryEmbedded { embedding, .. },
            ) => self.resolve_agent_recall_embedding(request, embedding, observed_at),
            (
                AgentRecallEffectPhase::Rerank { evidence, .. },
                AgentRecallHostOutcome::CandidatesReranked { rankings, .. },
            ) => resolve_rerank(evidence, rankings)
                .map(|evidence| self.finish_agent_recall_resolution(evidence)),
            (
                AgentRecallEffectPhase::Rerank { evidence, .. },
                AgentRecallHostOutcome::Failed { .. },
            ) => Ok(self.finish_agent_recall_resolution(evidence.clone())),
            (_, AgentRecallHostOutcome::Failed { .. }) => Ok(AgentRecallResolution::Finish {
                bounded_result: self.agent_recall_result_for_status("provider_unavailable", &[]),
                evidence: Vec::new(),
            }),
            (_, AgentRecallHostOutcome::Cancelled) => Ok(AgentRecallResolution::Cancelled),
            _ => Err(()),
        }
    }

    fn resolve_agent_recall_embedding(
        &self,
        request: &DurableAgentRecallEffectRequest,
        embedding: &pod0_application::RecallEmbeddingVector,
        observed_at: pod0_domain::UnixTimestampMilliseconds,
    ) -> Result<AgentRecallResolution, ()> {
        let interrupt = self.begin_recall_index_operation(request.cancellation_id);
        let candidates = self
            .recall_index
            .retrieve(
                embedding,
                &request.query.text,
                request.query.scope,
                u16::try_from(pod0_application::MAX_RECALL_CANDIDATES / 2).unwrap_or(u16::MAX),
                u16::try_from(pod0_application::MAX_RECALL_CANDIDATES / 2).unwrap_or(u16::MAX),
                u16::try_from(pod0_application::MAX_RECALL_CANDIDATES).unwrap_or(u16::MAX),
                interrupt.cancellation(),
            )
            .map_err(|_: RecallIndexError| ())?;
        let evidence = self.resolve_agent_candidates(request, &candidates)?;
        if evidence.is_empty() || request.reranker.is_none() {
            return Ok(self.finish_agent_recall_resolution(evidence));
        }
        let candidates = evidence
            .iter()
            .map(|item| RecallRerankDocument {
                span_id: item.span_id,
                excerpt: item.excerpt.clone(),
            })
            .collect();
        let phase = AgentRecallEffectPhase::Rerank {
            candidates,
            evidence,
        };
        let mut next = request.clone();
        next.phase = phase;
        next.request_id = pod0_application::agent_recall_request_id(
            next.turn_id,
            next.query.query_id,
            &next.phase,
        );
        next.deadline_at =
            pod0_domain::UnixTimestampMilliseconds::new(observed_at.value.saturating_add(30_000));
        Ok(AgentRecallResolution::Rerank { request: next })
    }

    fn resolve_agent_candidates(
        &self,
        request: &DurableAgentRecallEffectRequest,
        candidates: &[RecallIndexCandidate],
    ) -> Result<Vec<RecallEvidenceProjection>, ()> {
        let store = self.evidence_store.as_ref().ok_or(())?;
        resolve_candidates(store, request.query.scope, candidates, request.query.limit)
            .map_err(|_: CandidateResolutionError| ())
    }

    fn finish_agent_recall_resolution(
        &self,
        evidence: Vec<RecallEvidenceProjection>,
    ) -> AgentRecallResolution {
        AgentRecallResolution::Finish {
            bounded_result: self.agent_recall_result_for_evidence(&evidence),
            evidence,
        }
    }
}

fn resolve_rerank(
    evidence: &[RecallEvidenceProjection],
    rankings: &[pod0_application::RecallRerankObservation],
) -> Result<Vec<RecallEvidenceProjection>, ()> {
    let ranks = validate_rerank(evidence, rankings).ok_or(())?;
    let mut evidence = evidence.to_vec();
    for item in &mut evidence {
        item.score.rerank_rank = ranks.get(&item.span_id).copied();
    }
    evidence.sort_by_key(|item| item.score.rerank_rank.unwrap_or(u16::MAX));
    Ok(evidence)
}

fn stale(request_id: pod0_domain::HostRequestId) -> HostObservationReceipt {
    rejected_payload(request_id, HostObservationRejection::StaleWorkflow)
}

fn duplicate(request_id: pod0_domain::HostRequestId) -> HostObservationReceipt {
    rejected_payload(request_id, HostObservationRejection::Duplicate)
}

fn mismatched(request_id: pod0_domain::HostRequestId) -> HostObservationReceipt {
    rejected_payload(request_id, HostObservationRejection::MismatchedPayload)
}
