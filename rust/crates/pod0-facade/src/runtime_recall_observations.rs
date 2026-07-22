use pod0_application::{
    CoreFailureCode, HostFailureCode, HostObservation, HostRequest, MAX_RECALL_CANDIDATES,
    RecallPhase, RecallRerankDocument, RecallRerankObservation, RecallStage,
};
use pod0_domain::RecallQueryId;
use pod0_recall_index::{RecallIndexCandidate, RecallIndexError};

use crate::runtime_recall_rerank::validate_rerank;
use crate::runtime_recall_resolution::{CandidateResolutionError, resolve_candidates};
use crate::runtime_recall_state::{PendingRecall, RecallHostPhase};
use crate::runtime_state::FacadeState;

impl FacadeState {
    pub(super) fn finish_recall_observation(
        &mut self,
        pending: PendingRecall,
        observation: HostObservation,
    ) {
        match (pending.phase, observation) {
            (
                RecallHostPhase::Embedding,
                HostObservation::RecallQueryEmbedded { embedding, .. },
            ) => self.begin_retrieval(pending.query_id, embedding),
            (
                RecallHostPhase::Reranking,
                HostObservation::RecallCandidatesReranked { rankings, .. },
            ) => self.accept_rerank(pending.query_id, &rankings),
            (RecallHostPhase::Reranking, HostObservation::Failed { .. })
            | (RecallHostPhase::Reranking, HostObservation::Unsupported { .. }) => {
                self.finish_without_rerank(pending.query_id)
            }
            (
                RecallHostPhase::Embedding,
                HostObservation::Failed {
                    code: HostFailureCode::Unauthorized,
                    ..
                },
            ) => self.fail_recall(
                pending.query_id,
                RecallStage::ProviderUnavailable,
                CoreFailureCode::Unauthorized,
            ),
            (
                RecallHostPhase::Embedding,
                HostObservation::Failed {
                    code: HostFailureCode::ProviderUnavailable,
                    ..
                },
            )
            | (RecallHostPhase::Embedding, HostObservation::Unsupported { .. }) => self
                .fail_recall(
                    pending.query_id,
                    RecallStage::ProviderUnavailable,
                    CoreFailureCode::HostUnavailable,
                ),
            (RecallHostPhase::Embedding, HostObservation::Failed { .. }) => self.fail_recall(
                pending.query_id,
                RecallStage::ProviderUnavailable,
                CoreFailureCode::HostUnavailable,
            ),
            (_, HostObservation::Cancelled) => self.cancel_recall(pending.cancellation_id),
            _ => self.fail_recall(
                pending.query_id,
                RecallStage::Failed,
                CoreFailureCode::HostRejected,
            ),
        }
    }

    fn begin_retrieval(
        &mut self,
        query_id: RecallQueryId,
        embedding: pod0_application::RecallEmbeddingVector,
    ) {
        let Some(workflow) = self.recalls.get_mut(&query_id) else {
            return;
        };
        workflow.stage = RecallStage::Running {
            phase: RecallPhase::Retrieving,
        };
        let scope = workflow.scope;
        let cancellation_id = workflow.cancellation_id;
        let lexical_query = workflow.normalized_text.clone();
        let interrupt = self.begin_recall_index_operation(cancellation_id);
        let result = self.recall_index.retrieve(
            &embedding,
            &lexical_query,
            scope,
            u16::try_from(MAX_RECALL_CANDIDATES / 2).unwrap_or(u16::MAX),
            u16::try_from(MAX_RECALL_CANDIDATES / 2).unwrap_or(u16::MAX),
            u16::try_from(MAX_RECALL_CANDIDATES).unwrap_or(u16::MAX),
            interrupt.cancellation(),
        );
        match result {
            Ok(candidates) => self.accept_retrieval(query_id, candidates),
            Err(RecallIndexError::Cancelled) => {
                let cancellation_id = self.recalls[&query_id].cancellation_id;
                self.cancel_recall(cancellation_id);
            }
            Err(_) => self.fail_recall(
                query_id,
                RecallStage::IndexUnavailable,
                CoreFailureCode::StorageUnavailable,
            ),
        }
    }

    fn accept_retrieval(&mut self, query_id: RecallQueryId, candidates: Vec<RecallIndexCandidate>) {
        if candidates.is_empty() {
            self.complete_recall(query_id, RecallStage::NoEvidence, Vec::new());
            return;
        }
        let Some(workflow) = self.recalls.get(&query_id) else {
            return;
        };
        let Some(store) = &self.evidence_store else {
            self.fail_recall(
                query_id,
                RecallStage::IndexUnavailable,
                CoreFailureCode::StorageUnavailable,
            );
            return;
        };
        let evidence = match resolve_candidates(store, workflow.scope, &candidates, workflow.limit)
        {
            Ok(evidence) => evidence,
            Err(CandidateResolutionError::IndexUnavailable) => {
                self.fail_recall(
                    query_id,
                    RecallStage::IndexUnavailable,
                    CoreFailureCode::StorageUnavailable,
                );
                return;
            }
            Err(CandidateResolutionError::CorruptArtifact) => {
                self.fail_recall(
                    query_id,
                    RecallStage::CorruptArtifact,
                    CoreFailureCode::HostRejected,
                );
                return;
            }
        };
        if evidence.is_empty() {
            self.complete_recall(query_id, RecallStage::NoEvidence, Vec::new());
            return;
        }
        if !self.recall_configuration.reranker_enabled {
            let Some(workflow) = self.recalls.get_mut(&query_id) else {
                return;
            };
            workflow.evidence = evidence;
            self.finish_without_rerank(query_id);
            return;
        }
        let query = workflow.normalized_text.clone();
        let documents = evidence
            .iter()
            .map(|item| RecallRerankDocument {
                span_id: item.span_id,
                excerpt: item.excerpt.clone(),
            })
            .collect();
        let Some(workflow) = self.recalls.get_mut(&query_id) else {
            return;
        };
        workflow.stage = RecallStage::Running {
            phase: RecallPhase::Reranking,
        };
        workflow.evidence = evidence;
        self.queue_recall_request(
            query_id,
            RecallHostPhase::Reranking,
            HostRequest::RerankRecallCandidates {
                query_id,
                provider: self
                    .recall_configuration
                    .reranker_provider
                    .expect("enabled reranking has a validated provider"),
                model: self
                    .recall_configuration
                    .reranker_model
                    .clone()
                    .expect("enabled reranking has a validated model"),
                query,
                candidates: documents,
            },
        );
    }

    fn accept_rerank(&mut self, query_id: RecallQueryId, rankings: &[RecallRerankObservation]) {
        let Some(workflow) = self.recalls.get(&query_id) else {
            return;
        };
        let Some(ranks) = validate_rerank(&workflow.evidence, rankings) else {
            self.fail_recall(query_id, RecallStage::Failed, CoreFailureCode::HostRejected);
            return;
        };
        let mut evidence = workflow.evidence.clone();
        for item in &mut evidence {
            item.score.rerank_rank = ranks.get(&item.span_id).copied();
        }
        evidence.sort_by_key(|item| item.score.rerank_rank.unwrap_or(u16::MAX));
        self.complete_recall(query_id, RecallStage::Ready, evidence);
    }

    fn finish_without_rerank(&mut self, query_id: RecallQueryId) {
        let evidence = self
            .recalls
            .get(&query_id)
            .map(|workflow| workflow.evidence.clone())
            .unwrap_or_default();
        self.complete_recall(query_id, RecallStage::Ready, evidence);
    }
}
