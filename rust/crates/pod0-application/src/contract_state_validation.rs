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
            HostRequest::RetrieveRecallCandidates {
                query_id: expected, ..
            },
            HostObservation::RecallCandidatesRetrieved { query_id, .. },
        ) => expected == query_id,
        (
            HostRequest::RerankRecallCandidates {
                query_id: expected, ..
            },
            HostObservation::RecallCandidatesReranked { query_id, .. },
        ) => expected == query_id,
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
            HostRequest::RetrieveRecallCandidates {
                maximum_candidates, ..
            },
            HostObservation::RecallCandidatesRetrieved { candidates, .. },
        ) => {
            candidates.len() <= usize::from(*maximum_candidates)
                && candidates.len() <= crate::MAX_RECALL_CANDIDATES
        }
        (
            HostRequest::RerankRecallCandidates { candidates, .. },
            HostObservation::RecallCandidatesReranked { rankings, .. },
        ) => rankings.len() <= candidates.len() && rankings.len() <= crate::MAX_RECALL_EVIDENCE,
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
        | HostRequest::RetrieveRecallCandidates { .. }
        | HostRequest::RerankRecallCandidates { .. }
        | HostRequest::Unsupported { .. } => None,
    }
}
