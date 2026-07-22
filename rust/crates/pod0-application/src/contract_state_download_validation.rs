use crate::{HostObservation, HostRequest};

pub(super) fn download_observation_matches_request(
    request: &HostRequest,
    observation: &HostObservation,
) -> Option<bool> {
    let request_is_download = matches!(
        request,
        HostRequest::StartEpisodeDownload { .. }
            | HostRequest::CancelEpisodeDownload { .. }
            | HostRequest::RemoveEpisodeDownloadArtifact { .. }
    );
    let observation_is_download = matches!(
        observation,
        HostObservation::DownloadAccepted { .. }
            | HostObservation::DownloadStaged { .. }
            | HostObservation::DownloadCancelled { .. }
            | HostObservation::DownloadArtifactRemoved { .. }
    );
    if !request_is_download && !observation_is_download {
        return None;
    }
    Some(match (request, observation) {
        (
            HostRequest::StartEpisodeDownload {
                episode_id: expected_episode,
                intent_id: expected_intent,
                attempt_id: expected_attempt,
                ..
            },
            HostObservation::DownloadAccepted {
                episode_id,
                intent_id,
                attempt_id,
                ..
            }
            | HostObservation::DownloadStaged {
                episode_id,
                intent_id,
                attempt_id,
                ..
            }
            | HostObservation::DownloadCancelled {
                episode_id,
                intent_id,
                attempt_id,
            },
        ) => {
            expected_episode == episode_id
                && expected_intent == intent_id
                && expected_attempt == attempt_id
        }
        (
            HostRequest::CancelEpisodeDownload {
                episode_id: expected_episode,
                intent_id: expected_intent,
                attempt_id: expected_attempt,
                ..
            },
            HostObservation::DownloadCancelled {
                episode_id,
                intent_id,
                attempt_id,
            },
        ) => {
            expected_episode == episode_id
                && expected_intent == intent_id
                && expected_attempt == attempt_id
        }
        (
            HostRequest::RemoveEpisodeDownloadArtifact {
                episode_id: expected_episode,
                artifact_key: expected_artifact,
            },
            HostObservation::DownloadArtifactRemoved {
                episode_id,
                artifact_key,
            },
        ) => expected_episode == episode_id && expected_artifact == artifact_key,
        _ => false,
    })
}

pub(super) fn download_payload_is_bounded(
    request: &HostRequest,
    observation: &HostObservation,
) -> bool {
    if !matches!(
        request,
        HostRequest::StartEpisodeDownload { .. }
            | HostRequest::CancelEpisodeDownload { .. }
            | HostRequest::RemoveEpisodeDownloadArtifact { .. }
    ) {
        return true;
    }
    let request_is_bounded = match request {
        HostRequest::StartEpisodeDownload {
            input_version,
            enclosure_url,
            resume_key,
            ..
        } => {
            input_version.len() == 64
                && input_version.bytes().all(|byte| byte.is_ascii_hexdigit())
                && !enclosure_url.is_empty()
                && enclosure_url.len() <= crate::MAX_DOWNLOAD_ENCLOSURE_URL_BYTES
                && resume_key
                    .as_ref()
                    .is_none_or(|value| bounded_opaque_key(value))
        }
        HostRequest::CancelEpisodeDownload {
            external_task_key, ..
        } => external_task_key
            .as_ref()
            .is_none_or(|value| bounded_opaque_key(value)),
        HostRequest::RemoveEpisodeDownloadArtifact { artifact_key, .. } => {
            bounded_opaque_key(artifact_key)
        }
        _ => true,
    };
    request_is_bounded && observation_is_bounded(observation)
}

fn observation_is_bounded(observation: &HostObservation) -> bool {
    match observation {
        HostObservation::DownloadAccepted {
            external_task_key,
            resume_key,
            ..
        } => {
            bounded_opaque_key(external_task_key)
                && resume_key
                    .as_ref()
                    .is_none_or(|value| bounded_opaque_key(value))
        }
        HostObservation::DownloadStaged {
            staged_file_path,
            byte_count,
            ..
        } => bounded_opaque_key(staged_file_path) && *byte_count > 0,
        HostObservation::DownloadArtifactRemoved { artifact_key, .. } => {
            bounded_opaque_key(artifact_key)
        }
        HostObservation::Failed { safe_detail, .. } => safe_detail
            .as_ref()
            .is_none_or(|value| value.len() <= crate::MAX_DOWNLOAD_SAFE_DETAIL_BYTES),
        _ => true,
    }
}

fn bounded_opaque_key(value: &str) -> bool {
    !value.is_empty() && value.len() <= crate::MAX_DOWNLOAD_OPAQUE_KEY_BYTES
}
