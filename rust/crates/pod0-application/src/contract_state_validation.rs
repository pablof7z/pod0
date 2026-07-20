use crate::{HostObservation, HostRequest};

pub(super) fn observation_matches_request(
    request: &HostRequest,
    observation: &HostObservation,
) -> bool {
    if matches!(
        observation,
        HostObservation::Failed { .. } | HostObservation::Cancelled
    ) {
        return true;
    }
    match (request, observation) {
        (
            HostRequest::FetchFeed { .. },
            HostObservation::FeedBytesFetched { .. } | HostObservation::FeedNotModified { .. },
        ) => true,
        (
            HostRequest::ObservePlayback {
                episode_id: expected,
                ..
            },
            HostObservation::PlaybackObserved { value },
        ) => expected.is_none() || *expected == value.episode_id,
        (request, HostObservation::PlaybackObserved { value }) => {
            playback_request_episode_id(request)
                .is_some_and(|expected| value.episode_id == Some(expected))
        }
        (
            HostRequest::EmbedRecallQuery {
                query_id: expected, ..
            },
            HostObservation::RecallQueryEmbedded { query_id, .. },
        ) => expected == query_id,
        (
            HostRequest::EmbedRecallSpans {
                episode_id: expected_episode,
                generation_id: expected_generation,
                ..
            },
            HostObservation::RecallSpansEmbedded {
                episode_id,
                generation_id,
                ..
            },
        ) => expected_episode == episode_id && expected_generation == generation_id,
        (
            HostRequest::RerankRecallCandidates {
                query_id: expected, ..
            },
            HostObservation::RecallCandidatesReranked { query_id, .. },
        ) => expected == query_id,
        (
            HostRequest::FetchPublisherChapters {
                episode_id: expected,
                ..
            },
            HostObservation::PublisherChaptersFetched { episode_id, .. },
        ) => expected == episode_id,
        (
            HostRequest::RemoveLegacyRecallIndexArtifacts,
            HostObservation::LegacyRecallIndexArtifactsRemoved { removed_file_count },
        ) => *removed_file_count <= 3,
        (HostRequest::Unsupported { .. }, HostObservation::Unsupported { .. }) => true,
        _ => false,
    }
}

pub(super) fn recall_payload_is_bounded(
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
        _ => true,
    }
}

fn playback_request_episode_id(request: &HostRequest) -> Option<pod0_domain::EpisodeId> {
    match request {
        HostRequest::LoadMedia { episode_id, .. }
        | HostRequest::Play { episode_id, .. }
        | HostRequest::Pause { episode_id }
        | HostRequest::Seek { episode_id, .. }
        | HostRequest::SetRate { episode_id, .. }
        | HostRequest::ArmNativeTimer { episode_id, .. }
        | HostRequest::CancelNativeTimer { episode_id }
        | HostRequest::StopPlayback { episode_id } => Some(*episode_id),
        HostRequest::FetchFeed { .. }
        | HostRequest::ObservePlayback { .. }
        | HostRequest::EmbedRecallQuery { .. }
        | HostRequest::EmbedRecallSpans { .. }
        | HostRequest::RerankRecallCandidates { .. }
        | HostRequest::FetchPublisherChapters { .. }
        | HostRequest::RemoveLegacyRecallIndexArtifacts
        | HostRequest::Unsupported { .. } => None,
    }
}

fn bounded_span_embeddings(
    spans: &[crate::RecallEmbeddingInput],
    embeddings: &[crate::RecallSpanEmbeddingObservation],
    maximum_dimensions: u16,
) -> bool {
    use std::collections::BTreeSet;

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
