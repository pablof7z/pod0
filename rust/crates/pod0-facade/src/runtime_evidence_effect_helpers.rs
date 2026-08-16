use pod0_application::DurableEvidenceEmbeddingEffectRequest;
use pod0_domain::{
    CommandId, ContentDigest, EvidenceGenerationId, EvidenceSpanId, HostRequestId,
    TranscriptEvidenceArtifact,
};
use pod0_recall_index::RecallIndexSpan;
use sha2::{Digest as _, Sha256};

use crate::runtime_evidence_state::{EvidenceIndexCompletion, PendingEvidenceIndex};

pub(super) fn pending_from_effect(
    request: DurableEvidenceEmbeddingEffectRequest,
) -> PendingEvidenceIndex {
    PendingEvidenceIndex {
        command_id: request.command_id,
        cancellation_id: request.cancellation_id,
        episode_id: request.episode_id,
        generation_id: request.generation_id,
        expected_span_count: request.expected_span_count,
        requested_span_ids: request.spans.iter().map(|span| span.span_id).collect(),
        completion: EvidenceIndexCompletion::from_durable(request.completion),
    }
}

pub(super) fn evidence_effect_command_id(request_id: HostRequestId) -> CommandId {
    CommandId::from_bytes(request_id.into_bytes())
}

pub(super) fn evidence_effect_fingerprint(
    effect: &DurableEvidenceEmbeddingEffectRequest,
) -> ContentDigest {
    ContentDigest::from_bytes(
        Sha256::digest(serde_json::to_vec(effect).expect("typed evidence effect")).into(),
    )
}

pub(super) fn index_spans(artifact: &TranscriptEvidenceArtifact) -> Vec<RecallIndexSpan> {
    artifact
        .spans
        .iter()
        .map(|span| RecallIndexSpan {
            span_id: span.span_id,
            generation_id: artifact.generation_id,
            episode_id: span.episode_id,
            podcast_id: span.podcast_id,
            text: span.text.clone(),
        })
        .collect()
}

pub(super) fn evidence_index_request_id(
    command_id: CommandId,
    generation_id: EvidenceGenerationId,
    spans: &[EvidenceSpanId],
) -> HostRequestId {
    let mut hash = Sha256::new();
    hash.update(b"pod0-evidence-embedding-request-v2\0");
    hash.update(command_id.into_bytes());
    hash.update(generation_id.into_bytes());
    for span_id in spans {
        hash.update(span_id.into_bytes());
    }
    HostRequestId::from_bytes(hash.finalize()[..16].try_into().expect("digest prefix"))
}
