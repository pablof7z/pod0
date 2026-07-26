use crate::contract_state_agent_validation::agent_observation_matches;
use crate::contract_state_download_validation::download_observation_matches_request;
use crate::contract_state_playback_validation::playback_request_episode_id;
use crate::contract_state_scheduled_agent_validation::scheduled_agent_observation_matches;
use crate::contract_state_transcript_validation::transcript_observation_matches;
use crate::{HostObservation, HostRequest};

mod chapter;
mod recall;

pub(super) use chapter::chapter_model_payload_is_bounded;
pub(super) use recall::recall_payload_is_bounded;

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
    if let Some(matches) = download_observation_matches_request(request, observation) {
        return matches;
    }
    if let Some(matches) = scheduled_agent_observation_matches(request, observation) {
        return matches;
    }
    if let Some(matches) = agent_observation_matches(request, observation) {
        return matches;
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
            HostRequest::ExecuteChapterModel {
                episode_id: expected_episode,
                generation: expected_generation,
                submission_fence_id: expected_fence,
                ..
            },
            HostObservation::ChapterModelProviderAccepted {
                episode_id,
                generation,
                submission_fence_id,
                ..
            }
            | HostObservation::ChapterModelCompleted {
                episode_id,
                generation,
                submission_fence_id,
                ..
            }
            | HostObservation::ChapterModelFailed {
                episode_id,
                generation,
                submission_fence_id,
                ..
            },
        ) => {
            expected_episode == episode_id
                && expected_generation == generation
                && expected_fence == submission_fence_id
        }
        (
            HostRequest::RecoverChapterModelOperation {
                episode_id: expected_episode,
                generation: expected_generation,
                submission_fence_id: expected_fence,
                ..
            },
            HostObservation::ChapterModelProviderAccepted {
                episode_id,
                generation,
                submission_fence_id,
                ..
            }
            | HostObservation::ChapterModelCompleted {
                episode_id,
                generation,
                submission_fence_id,
                ..
            }
            | HostObservation::ChapterModelFailed {
                episode_id,
                generation,
                submission_fence_id,
                ..
            },
        ) => {
            expected_episode == episode_id
                && expected_generation == generation
                && expected_fence == submission_fence_id
        }
        (
            HostRequest::ScheduleCoreWake {
                reason: expected, ..
            },
            HostObservation::CoreWakeReached { reason },
        ) => expected == reason,
        (
            HostRequest::DeliverNewEpisodeNotification {
                occurrence_id: expected_occurrence,
                episode_id: expected_episode,
                ..
            },
            HostObservation::NewEpisodeNotificationDelivered {
                occurrence_id,
                episode_id,
            },
        ) => expected_occurrence == occurrence_id && expected_episode == episode_id,
        (
            HostRequest::ExecuteTranscriptCapability { capability },
            HostObservation::TranscriptCapabilityObserved { observation },
        ) => transcript_observation_matches(capability, observation),
        (
            HostRequest::RemoveLegacyRecallIndexArtifacts,
            HostObservation::LegacyRecallIndexArtifactsRemoved { removed_file_count },
        ) => *removed_file_count <= 3,
        (
            HostRequest::ProvisionNostrSignerCredential,
            HostObservation::NostrSignerCredentialReady { public_key_hex, .. },
        ) => crate::valid_lower_hex(public_key_hex, 64),
        (
            HostRequest::RestoreNostrSignerCredential {
                account_id: expected_account,
                expected_author_hex,
            },
            HostObservation::NostrSignerCredentialReady {
                account_id,
                public_key_hex,
            },
        ) => {
            expected_account == account_id
                && expected_author_hex == public_key_hex
                && crate::valid_lower_hex(public_key_hex, 64)
        }
        (HostRequest::SignNostrEvent { request }, HostObservation::NostrEventSigned { value }) => {
            request.account_id == value.account_id
                && request.event_id_hex == value.event_id_hex
                && crate::signature_observation_is_valid(value)
        }
        (
            HostRequest::DeleteNostrSignerCredential {
                account_id: expected,
            },
            HostObservation::NostrSignerCredentialDeleted { account_id },
        ) => expected == account_id,
        (HostRequest::Unsupported { .. }, HostObservation::Unsupported { .. }) => true,
        _ => false,
    }
}
