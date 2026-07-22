use std::collections::BTreeMap;

use pod0_application::{
    EvidenceCandidateObservation, MAX_RECALL_EXCERPT_BYTES, RecallEvidenceProjection, RecallScope,
    RecallScoreProjection, bounded_recall_text, rank_evidence,
};
use pod0_domain::{EpisodeId, TranscriptEvidenceArtifact};
use pod0_recall_index::RecallIndexCandidate;
use pod0_storage::EvidenceStore;

pub(super) fn resolve_candidates(
    store: &EvidenceStore,
    scope: RecallScope,
    candidates: &[RecallIndexCandidate],
    limit: u16,
) -> Result<Vec<RecallEvidenceProjection>, CandidateResolutionError> {
    let mut artifacts = BTreeMap::<EpisodeId, TranscriptEvidenceArtifact>::new();
    let mut spans = BTreeMap::new();
    let mut raw_ranks = Vec::with_capacity(candidates.len());
    for candidate in candidates {
        if let std::collections::btree_map::Entry::Vacant(entry) =
            artifacts.entry(candidate.episode_id)
        {
            let artifact = store
                .selected_artifact(candidate.episode_id)
                .map_err(|_| CandidateResolutionError::IndexUnavailable)?
                .ok_or(CandidateResolutionError::CorruptArtifact)?;
            if !scope_matches(scope, &artifact) {
                return Err(CandidateResolutionError::CorruptArtifact);
            }
            entry.insert(artifact);
        }
        let artifact = &artifacts[&candidate.episode_id];
        if artifact.generation_id != candidate.generation_id {
            return Err(CandidateResolutionError::CorruptArtifact);
        }
        let span = artifact
            .spans
            .iter()
            .find(|span| span.span_id == candidate.span_id)
            .cloned()
            .ok_or(CandidateResolutionError::CorruptArtifact)?;
        if spans
            .insert(candidate.span_id, (artifact.generation_id, span))
            .is_some()
        {
            return Err(CandidateResolutionError::CorruptArtifact);
        }
        raw_ranks.push(EvidenceCandidateObservation {
            span_id: candidate.span_id,
            vector_rank: candidate.vector_rank,
            lexical_rank: candidate.lexical_rank,
        });
    }
    rank_evidence(&raw_ranks, limit)
        .map_err(|_| CandidateResolutionError::CorruptArtifact)?
        .into_iter()
        .enumerate()
        .map(|(index, ranked)| {
            let (generation_id, span) = spans
                .remove(&ranked.span_id)
                .ok_or(CandidateResolutionError::CorruptArtifact)?;
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
                    base_rank: u16::try_from(index + 1)
                        .map_err(|_| CandidateResolutionError::CorruptArtifact)?,
                    rerank_rank: None,
                },
            })
        })
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CandidateResolutionError {
    IndexUnavailable,
    CorruptArtifact,
}

fn scope_matches(scope: RecallScope, artifact: &TranscriptEvidenceArtifact) -> bool {
    match scope {
        RecallScope::Library => true,
        RecallScope::Podcast { podcast_id } => artifact.version.podcast_id == podcast_id,
        RecallScope::Episode { episode_id } => artifact.version.episode_id == episode_id,
        RecallScope::Unsupported { .. } => false,
    }
}
