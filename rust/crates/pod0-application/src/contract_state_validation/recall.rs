use std::collections::BTreeSet;

use crate::contract_state_transcript_validation::transcript_observation_matches;
use crate::{HostObservation, HostRequest};

pub(crate) fn recall_payload_is_bounded(
    request: &HostRequest,
    observation: &HostObservation,
) -> bool {
    match (request, observation) {
        (
            HostRequest::EmbedRecallQuery {
                maximum_dimensions, ..
            },
            HostObservation::RecallQueryEmbedded { embedding, .. },
        ) => {
            !embedding.values.is_empty()
                && embedding.values.len() <= usize::from(*maximum_dimensions)
                && embedding.values.len() <= crate::MAX_RECALL_EMBEDDING_DIMENSIONS
        }
        (
            HostRequest::EmbedRecallSpans {
                spans,
                maximum_dimensions,
                ..
            },
            HostObservation::RecallSpansEmbedded { embeddings, .. },
        ) => bounded_span_embeddings(spans, embeddings, *maximum_dimensions),
        (
            HostRequest::RerankRecallCandidates { candidates, .. },
            HostObservation::RecallCandidatesReranked { rankings, .. },
        ) => rankings.len() <= candidates.len() && rankings.len() <= crate::MAX_RECALL_EVIDENCE,
        (
            HostRequest::FetchPublisherChapters {
                maximum_response_bytes,
                ..
            },
            HostObservation::PublisherChaptersFetched { bytes, .. },
        ) => u64::try_from(bytes.len()).is_ok_and(|size| size <= *maximum_response_bytes),
        (
            HostRequest::ExecuteChapterModel { execution, .. },
            HostObservation::ChapterModelCompleted { completion, .. },
        ) => u64::try_from(completion.completion.len())
            .is_ok_and(|size| size <= execution.maximum_completion_bytes),
        (
            HostRequest::RecoverChapterModelOperation {
                maximum_completion_bytes,
                ..
            },
            HostObservation::ChapterModelCompleted { completion, .. },
        ) => u64::try_from(completion.completion.len())
            .is_ok_and(|size| size <= *maximum_completion_bytes),
        (
            HostRequest::ExecuteTranscriptCapability { capability },
            HostObservation::TranscriptCapabilityObserved { observation },
        ) => transcript_observation_matches(capability, observation),
        (HostRequest::SignNostrEvent { request }, HostObservation::NostrEventSigned { value }) => {
            crate::signing_request_is_bounded(request)
                && crate::signature_observation_is_valid(value)
        }
        _ => true,
    }
}

fn bounded_span_embeddings(
    spans: &[crate::RecallEmbeddingInput],
    embeddings: &[crate::RecallSpanEmbeddingObservation],
    maximum_dimensions: u16,
) -> bool {
    let expected = spans
        .iter()
        .map(|span| span.span_id)
        .collect::<BTreeSet<_>>();
    let observed = embeddings
        .iter()
        .map(|embedding| embedding.span_id)
        .collect::<BTreeSet<_>>();
    !spans.is_empty()
        && spans.len() <= crate::MAX_RECALL_EMBEDDING_BATCH
        && spans.len() == embeddings.len()
        && expected.len() == spans.len()
        && observed.len() == embeddings.len()
        && expected == observed
        && spans.iter().all(|span| {
            !span.text.is_empty() && span.text.len() <= crate::MAX_RECALL_EMBEDDING_TEXT_BYTES
        })
        && embeddings.iter().all(|embedding| {
            !embedding.embedding.values.is_empty()
                && embedding.embedding.values.len() <= usize::from(maximum_dimensions)
                && embedding.embedding.values.len() <= crate::MAX_RECALL_EMBEDDING_DIMENSIONS
        })
}
