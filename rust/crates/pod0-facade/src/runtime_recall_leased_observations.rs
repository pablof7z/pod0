use pod0_application::{
    AgentRecallEffectPhase, AgentRecallHostOutcome, CoreFailureCode,
    DurableAgentRecallHostObservation, DurableEffectExecution, DurableRecallQueryEffectRequest,
    HostFailureCode, HostObservationReceipt, HostObservationRejection,
    LeasedHostObservationEnvelope, OperationResult, OperationStage, RecallEvidenceProjection,
    RecallQueryResolution, RecallRerankDocument, RecallStage,
};
use pod0_recall_index::{RecallIndexCandidate, RecallIndexError};

use crate::runtime_chapter_model_receipts::{persisted, retain};
use crate::runtime_leased_observations::rejected_payload;
use crate::runtime_recall_rerank::validate_rerank;
use crate::runtime_recall_resolution::{CandidateResolutionError, resolve_candidates};
use crate::runtime_recall_state::RecallWorkflow;
use crate::runtime_state::{FacadeState, failure};

impl FacadeState {
    pub(super) fn record_leased_query_recall_observation(
        &mut self,
        leased: LeasedHostObservationEnvelope,
        observation: DurableAgentRecallHostObservation,
    ) -> (bool, HostObservationReceipt) {
        let request_id = observation.request_id;
        let Some(store) = self.store.clone() else {
            return (false, retain(request_id));
        };
        let request = store
            .effect_request(leased.lease.intent_id)
            .ok()
            .flatten()
            .and_then(|value| match value.execution {
                DurableEffectExecution::RecallQuery { request } => Some(request),
                _ => None,
            });
        let Some(request) = request else {
            return (false, stale(request_id));
        };
        let resolution = match self.resolve_query_recall(&request, &observation) {
            Some(value) => value,
            None => return (false, mismatched(request_id)),
        };
        let (workflow, replayed) = match store.commit_recall_query_observation(
            leased.lease,
            observation,
            resolution,
            self.now(),
        ) {
            Ok(value) => value,
            Err(pod0_storage::StorageError::RevisionConflict) => return (false, stale(request_id)),
            Err(_) => return (false, retain(request_id)),
        };
        if replayed {
            return (false, duplicate(request_id));
        }
        let terminal = workflow.stage.is_terminal();
        let command_id = workflow.command_id;
        let query_id = workflow.query.query_id;
        let workflow_failure = workflow.failure.clone();
        let evidence_count = u16::try_from(workflow.evidence.len()).unwrap_or(u16::MAX);
        self.recalls.insert(
            query_id,
            RecallWorkflow {
                command_id: workflow.command_id,
                cancellation_id: workflow.cancellation_id,
                query_id: workflow.query.query_id,
                scope: workflow.query.scope,
                normalized_text: workflow.query.text,
                limit: workflow.query.limit,
                stage: workflow.stage,
                failure: workflow.failure,
                evidence: workflow.evidence,
            },
        );
        if terminal {
            if let Some(value) = workflow_failure {
                let stage = if value.code == CoreFailureCode::Cancelled {
                    OperationStage::Cancelled
                } else {
                    OperationStage::Failed
                };
                self.finish(command_id, stage, Some(value), None);
            } else {
                self.succeed(
                    command_id,
                    Some(OperationResult::RecallFinished {
                        query_id,
                        evidence_count,
                    }),
                );
            }
        }
        self.host_requests.retire(request_id);
        self.advance_revision();
        (true, persisted(request_id, terminal))
    }

    fn resolve_query_recall(
        &self,
        request: &DurableRecallQueryEffectRequest,
        observation: &DurableAgentRecallHostObservation,
    ) -> Option<RecallQueryResolution> {
        match (&request.phase, &observation.outcome) {
            (
                AgentRecallEffectPhase::EmbedQuery,
                AgentRecallHostOutcome::QueryEmbedded { embedding, .. },
            ) => {
                let interrupt = self.begin_recall_index_operation(request.cancellation_id);
                let candidates = match self.recall_index.retrieve(
                    embedding,
                    &request.query.text,
                    request.query.scope,
                    u16::try_from(pod0_application::MAX_RECALL_CANDIDATES / 2).ok()?,
                    u16::try_from(pod0_application::MAX_RECALL_CANDIDATES / 2).ok()?,
                    u16::try_from(pod0_application::MAX_RECALL_CANDIDATES).ok()?,
                    interrupt.cancellation(),
                ) {
                    Ok(value) => value,
                    Err(RecallIndexError::Cancelled) => return Some(cancelled()),
                    Err(_) => {
                        return Some(failed(
                            RecallStage::IndexUnavailable,
                            CoreFailureCode::StorageUnavailable,
                        ));
                    }
                };
                self.query_recall_from_candidates(request, &candidates, observation.observed_at)
            }
            (
                AgentRecallEffectPhase::Rerank { evidence, .. },
                AgentRecallHostOutcome::CandidatesReranked { rankings, .. },
            ) => {
                let Some(ranks) = validate_rerank(evidence, rankings) else {
                    return Some(failed(RecallStage::Failed, CoreFailureCode::HostRejected));
                };
                let mut evidence = evidence.clone();
                for item in &mut evidence {
                    item.score.rerank_rank = ranks.get(&item.span_id).copied();
                }
                evidence.sort_by_key(|item| item.score.rerank_rank.unwrap_or(u16::MAX));
                Some(finish(evidence))
            }
            (
                AgentRecallEffectPhase::Rerank { evidence, .. },
                AgentRecallHostOutcome::Failed { .. },
            ) => Some(finish(evidence.clone())),
            (_, AgentRecallHostOutcome::Failed { code, .. }) => Some(failed(
                RecallStage::ProviderUnavailable,
                provider_failure(*code),
            )),
            (_, AgentRecallHostOutcome::Cancelled) => Some(cancelled()),
            _ => None,
        }
    }

    fn query_recall_from_candidates(
        &self,
        request: &DurableRecallQueryEffectRequest,
        candidates: &[RecallIndexCandidate],
        observed_at: pod0_domain::UnixTimestampMilliseconds,
    ) -> Option<RecallQueryResolution> {
        let Some(store) = self.evidence_store.as_ref() else {
            return Some(failed(
                RecallStage::IndexUnavailable,
                CoreFailureCode::StorageUnavailable,
            ));
        };
        let evidence =
            match resolve_candidates(store, request.query.scope, candidates, request.query.limit) {
                Ok(value) => value,
                Err(CandidateResolutionError::IndexUnavailable) => {
                    return Some(failed(
                        RecallStage::IndexUnavailable,
                        CoreFailureCode::StorageUnavailable,
                    ));
                }
                Err(CandidateResolutionError::CorruptArtifact) => {
                    return Some(failed(
                        RecallStage::CorruptArtifact,
                        CoreFailureCode::HostRejected,
                    ));
                }
            };
        if evidence.is_empty() || request.reranker.is_none() {
            return Some(finish(evidence));
        }
        let documents = evidence
            .iter()
            .map(|item| RecallRerankDocument {
                span_id: item.span_id,
                excerpt: item.excerpt.clone(),
            })
            .collect();
        let phase = AgentRecallEffectPhase::Rerank {
            candidates: documents,
            evidence,
        };
        let mut next = request.clone();
        next.request_id = pod0_application::recall_query_request_id(next.query.query_id, &phase);
        next.phase = phase;
        next.deadline_at =
            pod0_domain::UnixTimestampMilliseconds::new(observed_at.value.saturating_add(30_000));
        Some(RecallQueryResolution::Rerank { request: next })
    }
}

fn finish(evidence: Vec<RecallEvidenceProjection>) -> RecallQueryResolution {
    RecallQueryResolution::Finish {
        stage: if evidence.is_empty() {
            RecallStage::NoEvidence
        } else {
            RecallStage::Ready
        },
        evidence,
        failure: None,
    }
}
fn failed(stage: RecallStage, code: CoreFailureCode) -> RecallQueryResolution {
    RecallQueryResolution::Finish {
        stage,
        evidence: Vec::new(),
        failure: Some(failure(code)),
    }
}
fn cancelled() -> RecallQueryResolution {
    failed(RecallStage::Cancelled, CoreFailureCode::Cancelled)
}
fn provider_failure(code: HostFailureCode) -> CoreFailureCode {
    match code {
        HostFailureCode::Unauthorized => CoreFailureCode::Unauthorized,
        HostFailureCode::PermissionDenied => CoreFailureCode::HostRejected,
        _ => CoreFailureCode::HostUnavailable,
    }
}
fn stale(request_id: pod0_domain::HostRequestId) -> HostObservationReceipt {
    rejected_payload(request_id, HostObservationRejection::StaleWorkflow)
}
fn mismatched(request_id: pod0_domain::HostRequestId) -> HostObservationReceipt {
    rejected_payload(request_id, HostObservationRejection::MismatchedPayload)
}
fn duplicate(request_id: pod0_domain::HostRequestId) -> HostObservationReceipt {
    rejected_payload(request_id, HostObservationRejection::Duplicate)
}
