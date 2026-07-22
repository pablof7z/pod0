use pod0_domain::{DownloadAttemptId, DownloadIntentId};

use crate::HostFailureCode;

use super::*;

fn request() -> HostRequestEnvelope {
    HostRequestEnvelope {
        request_id: HostRequestId::from_parts(0, 81),
        command_id: CommandId::from_parts(0, 82),
        cancellation_id: CancellationId::from_parts(0, 83),
        issued_revision: StateRevision::new(84),
        deadline_at: None,
        request: HostRequest::StartEpisodeDownload {
            episode_id: EpisodeId::from_parts(0, 85),
            intent_id: DownloadIntentId::from_parts(0, 86),
            attempt_id: DownloadAttemptId::from_parts(0, 87),
            input_version: "a".repeat(64),
            enclosure_url: "https://example.test/audio.mp3".into(),
            resume_key: None,
        },
    }
}

fn observation(value: HostObservation, sequence_number: u64) -> HostObservationEnvelope {
    HostObservationEnvelope {
        request_id: HostRequestId::from_parts(0, 81),
        cancellation_id: CancellationId::from_parts(0, 83),
        observed_request_revision: StateRevision::new(84),
        sequence_number,
        observed_at: UnixTimestampMilliseconds::new(1_000),
        observation: value,
    }
}

#[test]
fn accepted_download_keeps_request_open_for_one_terminal_observation() {
    let mut ledger = HostRequestLedger::default();
    assert!(ledger.register(request()));
    let accepted = observation(
        HostObservation::DownloadAccepted {
            episode_id: EpisodeId::from_parts(0, 85),
            intent_id: DownloadIntentId::from_parts(0, 86),
            attempt_id: DownloadAttemptId::from_parts(0, 87),
            external_task_key: "task-1".into(),
            resume_key: None,
        },
        1,
    );
    assert_eq!(
        ledger.accept_observation(&accepted),
        ObservationAcceptance::Accepted
    );

    let completed = observation(
        HostObservation::DownloadStaged {
            episode_id: EpisodeId::from_parts(0, 85),
            intent_id: DownloadIntentId::from_parts(0, 86),
            attempt_id: DownloadAttemptId::from_parts(0, 87),
            staged_file_path: "/tmp/download-attempt-87".into(),
            byte_count: 4_096,
        },
        2,
    );
    assert_eq!(
        ledger.accept_observation(&completed),
        ObservationAcceptance::Accepted
    );
    assert_eq!(
        ledger.accept_observation(&completed),
        ObservationAcceptance::Duplicate
    );
}

#[test]
fn download_observations_require_exact_intent_attempt_and_bounds() {
    let mut ledger = HostRequestLedger::default();
    assert!(ledger.register(request()));
    let mismatched = observation(
        HostObservation::DownloadStaged {
            episode_id: EpisodeId::from_parts(0, 85),
            intent_id: DownloadIntentId::from_parts(0, 86),
            attempt_id: DownloadAttemptId::from_parts(0, 88),
            staged_file_path: "/tmp/download-attempt-88".into(),
            byte_count: 4_096,
        },
        1,
    );
    assert_eq!(
        ledger.accept_observation(&mismatched),
        ObservationAcceptance::MismatchedPayload
    );

    let mismatched_episode = observation(
        HostObservation::DownloadStaged {
            episode_id: EpisodeId::from_parts(0, 89),
            intent_id: DownloadIntentId::from_parts(0, 86),
            attempt_id: DownloadAttemptId::from_parts(0, 87),
            staged_file_path: "/tmp/download-attempt-87".into(),
            byte_count: 4_096,
        },
        1,
    );
    assert_eq!(
        ledger.accept_observation(&mismatched_episode),
        ObservationAcceptance::MismatchedPayload
    );

    let mismatched_intent = observation(
        HostObservation::DownloadAccepted {
            episode_id: EpisodeId::from_parts(0, 85),
            intent_id: DownloadIntentId::from_parts(0, 90),
            attempt_id: DownloadAttemptId::from_parts(0, 87),
            external_task_key: "task-1".into(),
            resume_key: None,
        },
        1,
    );
    assert_eq!(
        ledger.accept_observation(&mismatched_intent),
        ObservationAcceptance::MismatchedPayload
    );

    let unsupported = observation(HostObservation::Unsupported { wire_code: 91 }, 1);
    assert_eq!(
        ledger.accept_observation(&unsupported),
        ObservationAcceptance::MismatchedPayload
    );

    let oversized = observation(
        HostObservation::DownloadAccepted {
            episode_id: EpisodeId::from_parts(0, 85),
            intent_id: DownloadIntentId::from_parts(0, 86),
            attempt_id: DownloadAttemptId::from_parts(0, 87),
            external_task_key: "x".repeat(crate::MAX_DOWNLOAD_OPAQUE_KEY_BYTES + 1),
            resume_key: None,
        },
        1,
    );
    assert_eq!(
        ledger.accept_observation(&oversized),
        ObservationAcceptance::PayloadTooLarge
    );

    let empty_artifact = observation(
        HostObservation::DownloadStaged {
            episode_id: EpisodeId::from_parts(0, 85),
            intent_id: DownloadIntentId::from_parts(0, 86),
            attempt_id: DownloadAttemptId::from_parts(0, 87),
            staged_file_path: "/tmp/download-attempt-87".into(),
            byte_count: 0,
        },
        1,
    );
    assert_eq!(
        ledger.accept_observation(&empty_artifact),
        ObservationAcceptance::PayloadTooLarge
    );
}

#[test]
fn download_bounds_do_not_change_unrelated_host_failure_limits() {
    let mut ledger = HostRequestLedger::default();
    let mut feed_request = request();
    feed_request.request = HostRequest::FetchFeed {
        feed_url: "https://example.test/feed.xml".into(),
        entity_tag: None,
        last_modified: None,
        maximum_response_bytes: crate::MAX_FEED_RESPONSE_BYTES,
    };
    assert!(ledger.register(feed_request));
    assert_eq!(
        ledger.accept_observation(&observation(
            HostObservation::Failed {
                code: HostFailureCode::InvalidResponse,
                safe_detail: Some("x".repeat(crate::MAX_DOWNLOAD_SAFE_DETAIL_BYTES + 1)),
            },
            1,
        )),
        ObservationAcceptance::Accepted
    );
}

#[test]
fn cancellation_and_removal_observations_match_their_exact_request() {
    let mut cancellation = HostRequestLedger::default();
    let mut cancel_request = request();
    cancel_request.request = HostRequest::CancelEpisodeDownload {
        episode_id: EpisodeId::from_parts(0, 85),
        intent_id: DownloadIntentId::from_parts(0, 86),
        attempt_id: DownloadAttemptId::from_parts(0, 87),
        external_task_key: Some("task-1".into()),
    };
    assert!(cancellation.register(cancel_request));
    assert_eq!(
        cancellation.accept_observation(&observation(
            HostObservation::DownloadCancelled {
                episode_id: EpisodeId::from_parts(0, 85),
                intent_id: DownloadIntentId::from_parts(0, 86),
                attempt_id: DownloadAttemptId::from_parts(0, 87),
            },
            1,
        )),
        ObservationAcceptance::Accepted
    );

    let mut removal = HostRequestLedger::default();
    let mut remove_request = request();
    remove_request.request = HostRequest::RemoveEpisodeDownloadArtifact {
        episode_id: EpisodeId::from_parts(0, 85),
        artifact_key: "download/85".into(),
    };
    assert!(removal.register(remove_request));
    assert_eq!(
        removal.accept_observation(&observation(
            HostObservation::DownloadArtifactRemoved {
                episode_id: EpisodeId::from_parts(0, 85),
                artifact_key: "download/85".into(),
            },
            1,
        )),
        ObservationAcceptance::Accepted
    );
}
