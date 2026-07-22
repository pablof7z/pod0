use pod0_domain::{AutoDownloadMode, AutoDownloadPolicy, EpisodeId};

use super::*;

fn policy(mode: AutoDownloadMode, wifi_only: bool) -> AutoDownloadPolicy {
    AutoDownloadPolicy { mode, wifi_only }
}

fn environment(
    network: DownloadNetworkState,
    capacity: Option<u64>,
) -> DownloadEnvironmentObservation {
    DownloadEnvironmentObservation {
        network,
        available_capacity_bytes: capacity,
    }
}

#[test]
fn automatic_download_respects_disabled_and_wifi_only_policy() {
    assert_eq!(
        evaluate_download_admission(
            DownloadIntentOrigin::Automatic,
            policy(AutoDownloadMode::Off, false),
            environment(DownloadNetworkState::Wifi, None),
        ),
        DownloadAdmissionDecision::Obsolete
    );
    assert_eq!(
        evaluate_download_admission(
            DownloadIntentOrigin::Automatic,
            policy(AutoDownloadMode::AllNew, true),
            environment(DownloadNetworkState::Other, None),
        ),
        DownloadAdmissionDecision::Wait {
            reason: DownloadWaitReason::WifiRequired,
        }
    );
    assert_eq!(
        evaluate_download_admission(
            DownloadIntentOrigin::Automatic,
            policy(AutoDownloadMode::AllNew, true),
            environment(DownloadNetworkState::Unknown, None),
        ),
        DownloadAdmissionDecision::Wait {
            reason: DownloadWaitReason::WifiRequired,
        }
    );
    assert_eq!(
        evaluate_download_admission(
            DownloadIntentOrigin::Automatic,
            policy(AutoDownloadMode::AllNew, false),
            environment(DownloadNetworkState::Unavailable, None),
        ),
        DownloadAdmissionDecision::Wait {
            reason: DownloadWaitReason::NetworkUnavailable,
        }
    );
    assert_eq!(
        evaluate_download_admission(
            DownloadIntentOrigin::Automatic,
            policy(AutoDownloadMode::AllNew, true),
            environment(DownloadNetworkState::Wifi, None),
        ),
        DownloadAdmissionDecision::Admit
    );
}

#[test]
fn manual_and_playback_intents_still_wait_for_network_and_capacity() {
    let automatic_off = policy(AutoDownloadMode::Off, true);
    assert_eq!(
        evaluate_download_admission(
            DownloadIntentOrigin::User,
            automatic_off,
            environment(DownloadNetworkState::Unknown, None),
        ),
        DownloadAdmissionDecision::Wait {
            reason: DownloadWaitReason::NetworkUnknown,
        }
    );
    assert_eq!(
        evaluate_download_admission(
            DownloadIntentOrigin::Playback,
            automatic_off,
            environment(DownloadNetworkState::Unavailable, None),
        ),
        DownloadAdmissionDecision::Wait {
            reason: DownloadWaitReason::NetworkUnavailable,
        }
    );
    assert_eq!(
        evaluate_download_admission(
            DownloadIntentOrigin::User,
            automatic_off,
            environment(
                DownloadNetworkState::Wifi,
                Some(DOWNLOAD_MINIMUM_FREE_CAPACITY_BYTES - 1),
            ),
        ),
        DownloadAdmissionDecision::Wait {
            reason: DownloadWaitReason::InsufficientStorage,
        }
    );
    assert_eq!(
        evaluate_download_admission(
            DownloadIntentOrigin::Playback,
            automatic_off,
            environment(
                DownloadNetworkState::Other,
                Some(DOWNLOAD_MINIMUM_FREE_CAPACITY_BYTES),
            ),
        ),
        DownloadAdmissionDecision::Admit
    );
}

#[test]
fn unsupported_origin_and_environment_fail_closed() {
    let automatic = policy(AutoDownloadMode::AllNew, false);
    assert_eq!(
        evaluate_download_admission(
            DownloadIntentOrigin::Unsupported { wire_code: 81 },
            automatic,
            environment(DownloadNetworkState::Wifi, None),
        ),
        DownloadAdmissionDecision::Obsolete
    );
    assert_eq!(
        evaluate_download_admission(
            DownloadIntentOrigin::User,
            automatic,
            environment(DownloadNetworkState::Unsupported { wire_code: 82 }, None,),
        ),
        DownloadAdmissionDecision::Wait {
            reason: DownloadWaitReason::UnsupportedEnvironment { wire_code: 82 },
        }
    );
}

#[test]
fn input_intent_and_attempt_identity_are_deterministic_and_fenced() {
    let input = download_input_version(
        "https://example.test/audio.mp3",
        Some("audio/mpeg"),
        Some(7_200_000),
    )
    .unwrap();
    assert_eq!(
        input,
        "0719befd74acfee7dafaa6454377766187795a9e395f4a4ba7ecffa7c5e93ed9"
    );
    assert_eq!(
        download_input_version(
            "https://example.test/audio.mp3",
            Some("audio/mpeg"),
            Some(7_200_000),
        )
        .as_deref(),
        Some(input.as_str())
    );
    assert_ne!(
        download_input_version(
            "https://example.test/audio-v2.mp3",
            Some("audio/mpeg"),
            Some(7_200_000),
        )
        .unwrap(),
        input
    );

    let intent = download_intent_id(EpisodeId::from_parts(4, 9), &input).unwrap();
    assert_eq!(
        intent,
        pod0_domain::DownloadIntentId::from_parts(0xf4b5_f8e6_05de_15be, 0x7945_3785_c58b_ded4)
    );
    assert_eq!(
        download_intent_id(EpisodeId::from_parts(4, 9), &input),
        Some(intent)
    );
    assert_ne!(
        download_intent_id(EpisodeId::from_parts(4, 10), &input),
        Some(intent)
    );
    assert_eq!(download_attempt_id(intent, 0), None);
    assert_eq!(
        download_attempt_id(intent, 1),
        Some(pod0_domain::DownloadAttemptId::from_parts(
            0x3cc6_88ca_14ed_8cea,
            0x1c30_0220_4561_476e,
        ))
    );
    assert_ne!(
        download_attempt_id(intent, 1),
        download_attempt_id(intent, 2)
    );
}

#[test]
fn retry_timing_uses_injected_time_and_saturates() {
    assert_eq!(
        download_retry_not_before(pod0_domain::UnixTimestampMilliseconds::new(1_000)),
        pod0_domain::UnixTimestampMilliseconds::new(1_000 + DOWNLOAD_RETRY_DELAY_MILLISECONDS)
    );
    assert_eq!(
        download_retry_not_before(pod0_domain::UnixTimestampMilliseconds::new(i64::MAX - 1)),
        pod0_domain::UnixTimestampMilliseconds::new(i64::MAX)
    );
}

#[test]
fn invalid_media_and_input_versions_are_rejected() {
    assert_eq!(
        download_input_version(" ftp://example.test/a ", None, None),
        None
    );
    assert_eq!(
        download_intent_id(EpisodeId::from_parts(1, 2), "not-a-digest"),
        None
    );
}

#[test]
fn workflow_projection_is_bounded_by_the_open_view() {
    let input = download_input_version("https://example.test/a.mp3", None, None).unwrap();
    let mut projection = DownloadWorkflowsProjection {
        workflows: (0_u64..250)
            .map(|index| {
                let episode_id = EpisodeId::from_parts(0, index + 1);
                DownloadWorkflowProjection {
                    episode_id,
                    intent_id: download_intent_id(episode_id, &input).unwrap(),
                    input_version: input.clone(),
                    origin: DownloadIntentOrigin::User,
                    desired_state: DownloadDesiredState::Present,
                    stage: DownloadWorkflowStage::Requested,
                    workflow_revision: pod0_domain::StateRevision::new(index),
                    attempt: 0,
                    attempt_id: None,
                    request_id: None,
                    not_before: None,
                    failure: None,
                    updated_at: pod0_domain::UnixTimestampMilliseconds::new(index as i64),
                    allowed_actions: DownloadWorkflowAllowedActions {
                        can_retry: false,
                        can_cancel: true,
                        can_remove: false,
                    },
                }
            })
            .collect(),
        has_more: false,
        failure: None,
    };

    projection.enforce_bounds(10, usize::MAX);

    assert_eq!(
        projection.workflows.len(),
        usize::from(MAX_PROJECTION_ITEMS)
    );
    assert_eq!(
        projection.workflows[0].episode_id,
        EpisodeId::from_parts(0, 11)
    );
    assert!(projection.has_more);
}
