use crate::runtime_playback_test_support::{
    PlaybackFixture, add_external_episode, dispatch, playback, record_observation, record_playback,
};
use crate::*;

#[test]
fn selecting_loaded_episode_is_idempotent_while_playing() {
    let fixture = PlaybackFixture::new();
    fixture.dispatch(
        40,
        PlaybackCommand::Select {
            episode_id: fixture.episode_id,
            segment: None,
            label: None,
        },
    );
    let initial = fixture.facade.next_host_requests(u16::MAX);
    let load = initial
        .iter()
        .find(|request| matches!(request.request, HostRequest::LoadMedia { .. }))
        .unwrap();
    record_observation(
        &fixture.facade,
        load,
        0,
        1_000,
        host_observation(fixture.episode_id, PlaybackHostState::Prepared, 0, false),
    );
    fixture.dispatch(
        41,
        PlaybackCommand::Play {
            transcript_configuration: None,
        },
    );
    let play = fixture
        .facade
        .next_host_requests(u16::MAX)
        .into_iter()
        .find(|request| matches!(request.request, HostRequest::Play { .. }))
        .unwrap();
    record_observation(
        &fixture.facade,
        &play,
        0,
        1_001,
        host_observation(fixture.episode_id, PlaybackHostState::Playing, 1_000, false),
    );

    fixture.dispatch(
        42,
        PlaybackCommand::Select {
            episode_id: fixture.episode_id,
            segment: None,
            label: None,
        },
    );

    assert!(fixture.facade.next_host_requests(u16::MAX).is_empty());
    assert_eq!(
        fixture.playback().current.unwrap().policy_state,
        PlaybackPolicyState::Playing
    );
}

#[test]
fn failed_media_is_reloaded_before_playing_again() {
    let fixture = PlaybackFixture::new();
    fixture.dispatch(50, PlaybackCommand::Restore);
    let load = fixture
        .facade
        .next_host_requests(u16::MAX)
        .into_iter()
        .find(|request| matches!(request.request, HostRequest::LoadMedia { .. }))
        .unwrap();
    fixture
        .facade
        .record_host_observation(HostObservationEnvelope {
            request_id: load.request_id,
            cancellation_id: load.cancellation_id,
            observed_request_revision: load.issued_revision,
            sequence_number: 0,
            observed_at: UnixTimestampMilliseconds::new(2_000),
            observation: HostObservation::Failed {
                code: HostFailureCode::MediaUnavailable,
                safe_detail: None,
            },
        });

    fixture.dispatch(
        51,
        PlaybackCommand::Play {
            transcript_configuration: None,
        },
    );
    let effects = fixture.facade.next_host_requests(u16::MAX);
    assert!(effects.iter().any(|request| matches!(
        request.request,
        HostRequest::LoadMedia { episode_id, .. } if episode_id == fixture.episode_id
    )));
    assert!(effects.iter().any(|request| matches!(
        request.request,
        HostRequest::Play { episode_id, .. } if episode_id == fixture.episode_id
    )));
}

#[test]
fn adjacent_segments_from_one_episode_advance_without_stopping() {
    let fixture = PlaybackFixture::new();
    let first = PlaybackSegment {
        start_position_milliseconds: Some(10_000),
        end_position_milliseconds: Some(20_000),
    };
    let second = PlaybackSegment {
        start_position_milliseconds: Some(30_000),
        end_position_milliseconds: Some(40_000),
    };
    fixture.dispatch(
        60,
        PlaybackCommand::Select {
            episode_id: fixture.episode_id,
            segment: Some(first),
            label: Some("First".to_owned()),
        },
    );
    fixture.dispatch(
        61,
        PlaybackCommand::Enqueue {
            entry: QueueEntry {
                queue_entry_id: QueueEntryId::from_parts(12, 60),
                episode_id: fixture.episode_id,
                segment: Some(second),
                label: Some("Second".to_owned()),
            },
            placement: QueuePlacement::Back,
        },
    );
    let stream = fixture
        .facade
        .next_host_requests(u16::MAX)
        .into_iter()
        .find(|request| matches!(request.request, HostRequest::ObservePlayback { .. }))
        .unwrap();

    record_playback(
        &fixture.facade,
        &stream,
        1,
        3_000,
        20_000,
        false,
        PlaybackInterruption::None,
    );

    let current = fixture.playback().current.unwrap();
    assert_eq!(current.episode_id, fixture.episode_id);
    assert_eq!(current.segment, Some(second));
    let effects = fixture.facade.next_host_requests(u16::MAX);
    assert!(effects.iter().any(|request| matches!(
        request.request,
        HostRequest::LoadMedia { episode_id, start_position_milliseconds: 30_000, .. }
            if episode_id == fixture.episode_id
    )));
    assert!(effects.iter().any(|request| matches!(
        request.request,
        HostRequest::Play { episode_id, .. } if episode_id == fixture.episode_id
    )));
}

#[test]
fn empty_manual_advance_does_not_restart_active_media() {
    let fixture = PlaybackFixture::new();
    fixture.dispatch(70, PlaybackCommand::Restore);
    let _ = fixture.facade.next_host_requests(u16::MAX);

    fixture.dispatch(71, PlaybackCommand::AdvanceQueue);

    assert!(fixture.facade.next_host_requests(u16::MAX).is_empty());
    assert_eq!(
        fixture.playback().current.unwrap().episode_id,
        fixture.episode_id
    );
}

#[test]
fn restart_restores_queue_resume_rate_and_clears_session_timer() {
    let fixture = PlaybackFixture::new();
    let second = add_external_episode(&fixture, 80);
    fixture.dispatch(
        81,
        PlaybackCommand::Select {
            episode_id: fixture.episode_id,
            segment: None,
            label: None,
        },
    );
    fixture.dispatch(
        82,
        PlaybackCommand::Enqueue {
            entry: QueueEntry {
                queue_entry_id: QueueEntryId::from_parts(12, 80),
                episode_id: second,
                segment: None,
                label: None,
            },
            placement: QueuePlacement::Back,
        },
    );
    fixture.dispatch(
        83,
        PlaybackCommand::SetRate {
            rate: PlaybackRatePermille { value: 1_250 },
        },
    );
    fixture.dispatch(
        84,
        PlaybackCommand::SetSleepTimer {
            mode: PlaybackSleepMode::Duration {
                duration_milliseconds: 900_000,
            },
        },
    );
    fixture.dispatch(
        85,
        PlaybackCommand::Checkpoint {
            episode_id: fixture.episode_id,
            position_milliseconds: 47_000,
        },
    );
    let target = fixture.target.to_string_lossy().into_owned();
    drop(fixture.facade);

    let reopened = Pod0Facade::open(target).unwrap();
    let restored = playback(&reopened);
    let current = restored.current.as_ref().unwrap();
    assert_eq!(current.durable_resume_position_milliseconds, 47_000);
    assert_eq!(restored.queue[0].episode_id, second);
    assert_eq!(restored.rate.value, 1_250);
    assert_eq!(restored.sleep_mode, PlaybackSleepMode::Off);
    assert_eq!(current.policy_state, PlaybackPolicyState::Paused);

    dispatch(&reopened, 86, PlaybackCommand::Restore);
    let effects = reopened.next_host_requests(u16::MAX);
    assert!(effects.iter().any(|request| matches!(
        request.request,
        HostRequest::LoadMedia {
            start_position_milliseconds: 47_000,
            ..
        }
    )));
    assert!(effects.iter().any(|request| matches!(
        request.request,
        HostRequest::SetRate {
            rate: PlaybackRatePermille { value: 1_250 },
            ..
        }
    )));
    assert!(
        !effects
            .iter()
            .any(|request| matches!(request.request, HostRequest::Play { .. }))
    );
}

fn host_observation(
    episode_id: EpisodeId,
    state: PlaybackHostState,
    position_milliseconds: u64,
    ended: bool,
) -> PlaybackLifecycleObservation {
    PlaybackLifecycleObservation {
        episode_id: Some(episode_id),
        state,
        position_milliseconds,
        duration_milliseconds: 120_500,
        route: PlaybackAudioRoute::BuiltIn,
        interruption: PlaybackInterruption::None,
        ended,
    }
}
