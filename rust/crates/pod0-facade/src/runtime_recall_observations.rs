use std::collections::{BTreeMap, BTreeSet};

use pod0_application::{
    CoreFailureCode, EvidenceCandidateObservation, HostObservation, HostRequest,
    MAX_RECALL_CANDIDATES, MAX_RECALL_EVIDENCE, MAX_RECALL_EXCERPT_BYTES,
    RecallCandidateObservation, RecallEvidenceProjection, RecallPhase, RecallRerankDocument,
    RecallRerankObservation, RecallScope, RecallScoreProjection, RecallStage, bounded_recall_text,
    rank_evidence,
};
use pod0_domain::{EpisodeId, EvidenceSpanId, RecallQueryId, TranscriptEvidenceArtifact};
use pod0_storage::EvidenceStore;

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
                RecallHostPhase::Retrieval,
                HostObservation::RecallCandidatesRetrieved { candidates, .. },
            ) => self.accept_retrieval(pending.query_id, candidates),
            (
                RecallHostPhase::Reranking,
                HostObservation::RecallCandidatesReranked { rankings, .. },
            ) => self.accept_rerank(pending.query_id, &rankings),
            (RecallHostPhase::Reranking, HostObservation::Failed { .. })
            | (RecallHostPhase::Reranking, HostObservation::Unsupported { .. }) => {
                self.finish_without_rerank(pending.query_id)
            }
            (_, HostObservation::Failed { .. }) | (_, HostObservation::Unsupported { .. }) => self
                .fail_recall(
                    pending.query_id,
                    RecallStage::IndexUnavailable,
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
        let lexical_query = workflow.normalized_text.clone();
        self.queue_recall_request(
            query_id,
            RecallHostPhase::Retrieval,
            HostRequest::RetrieveRecallCandidates {
                query_id,
                scope,
                lexical_query,
                embedding,
                maximum_candidates: u16::try_from(MAX_RECALL_CANDIDATES).unwrap_or(u16::MAX),
            },
        );
    }

    fn accept_retrieval(
        &mut self,
        query_id: RecallQueryId,
        candidates: Vec<RecallCandidateObservation>,
    ) {
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
        let evidence = resolve_candidates(store, workflow.scope, &candidates, workflow.limit);
        let Ok(evidence) = evidence else {
            self.fail_recall(query_id, RecallStage::Failed, CoreFailureCode::HostRejected);
            return;
        };
        if evidence.is_empty() {
            self.complete_recall(query_id, RecallStage::NoEvidence, Vec::new());
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

fn resolve_candidates(
    store: &EvidenceStore,
    scope: RecallScope,
    candidates: &[RecallCandidateObservation],
    limit: u16,
) -> Result<Vec<RecallEvidenceProjection>, ()> {
    let mut artifacts = BTreeMap::<EpisodeId, TranscriptEvidenceArtifact>::new();
    let mut spans = BTreeMap::new();
    let mut raw_ranks = Vec::with_capacity(candidates.len());
    for candidate in candidates {
        if let std::collections::btree_map::Entry::Vacant(entry) =
            artifacts.entry(candidate.episode_id)
        {
            let artifact = store
                .selected_artifact(candidate.episode_id)
                .map_err(|_| ())?
                .ok_or(())?;
            if !scope_matches(scope, &artifact) {
                return Err(());
            }
            entry.insert(artifact);
        }
        let artifact = &artifacts[&candidate.episode_id];
        if artifact.generation_id != candidate.generation_id {
            return Err(());
        }
        let span = artifact
            .spans
            .iter()
            .find(|span| span.span_id == candidate.span_id)
            .cloned()
            .ok_or(())?;
        if spans
            .insert(candidate.span_id, (artifact.generation_id, span))
            .is_some()
        {
            return Err(());
        }
        raw_ranks.push(EvidenceCandidateObservation {
            span_id: candidate.span_id,
            vector_rank: candidate.vector_rank,
            lexical_rank: candidate.lexical_rank,
        });
    }
    rank_evidence(&raw_ranks, limit)
        .map_err(|_| ())?
        .into_iter()
        .enumerate()
        .map(|(index, ranked)| {
            let (generation_id, span) = spans.remove(&ranked.span_id).ok_or(())?;
            Ok(RecallEvidenceProjection {
                episode_id: span.episode_id,
                podcast_id: span.podcast_id,
                generation_id,
                transcript_version_id: span.transcript_version_id,
                transcript_content_digest: span.transcript_content_digest,
                span_id: span.span_id,
                first_segment_id: span.first_segment_id,
                last_segment_id: span.last_segment_id,
                start_segment_ordinal: span.start_segment_ordinal,
                end_segment_ordinal_exclusive: span.end_segment_ordinal_exclusive,
                start_milliseconds: span.start_milliseconds,
                end_milliseconds: span.end_milliseconds,
                excerpt: bounded_recall_text(&span.text, MAX_RECALL_EXCERPT_BYTES),
                speaker_id: span.speaker_id,
                provenance: span.provenance,
                score: RecallScoreProjection {
                    vector_rrf_units: ranked.score.vector_rrf_units,
                    lexical_rrf_units: ranked.score.lexical_rrf_units,
                    total_rrf_units: ranked.score.total_rrf_units,
                    base_rank: u16::try_from(index + 1).map_err(|_| ())?,
                    rerank_rank: None,
                },
            })
        })
        .collect()
}

fn scope_matches(scope: RecallScope, artifact: &TranscriptEvidenceArtifact) -> bool {
    match scope {
        RecallScope::Library => true,
        RecallScope::Podcast { podcast_id } => artifact.version.podcast_id == podcast_id,
        RecallScope::Episode { episode_id } => artifact.version.episode_id == episode_id,
        RecallScope::Unsupported { .. } => false,
    }
}

fn validate_rerank(
    evidence: &[RecallEvidenceProjection],
    rankings: &[RecallRerankObservation],
) -> Option<BTreeMap<EvidenceSpanId, u16>> {
    if evidence.len() != rankings.len() || evidence.len() > MAX_RECALL_EVIDENCE {
        return None;
    }
    let expected = evidence
        .iter()
        .map(|item| item.span_id)
        .collect::<BTreeSet<_>>();
    let observed = rankings
        .iter()
        .map(|item| item.span_id)
        .collect::<BTreeSet<_>>();
    let ranks = rankings
        .iter()
        .map(|item| item.rank)
        .collect::<BTreeSet<_>>();
    if expected != observed
        || ranks.len() != rankings.len()
        || !ranks
            .iter()
            .enumerate()
            .all(|(index, rank)| usize::from(*rank) == index + 1)
    {
        return None;
    }
    Some(
        rankings
            .iter()
            .map(|item| (item.span_id, item.rank))
            .collect(),
    )
}
