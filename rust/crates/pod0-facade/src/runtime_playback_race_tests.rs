use crate::runtime_playback_test_support::{
    PlaybackFixture, add_external_episode, library_request, record_observation,
};
use crate::*;

#[test]
fn ended_observation_for_replaced_episode_cannot_complete_new_active_item() {
    let fixture = PlaybackFixture::new();
    let replacement = add_external_episode(&fixture, 90);
    fixture.dispatch(91, PlaybackCommand::Restore);
    let stream = fixture
        .facade
        .next_host_requests(u16::MAX)
        .into_iter()
        .find(|request| matches!(request.request, HostRequest::ObservePlayback { .. }))
        .unwrap();
    fixture.dispatch(
        92,
        PlaybackCommand::Select {
            episode_id: replacement,
            segment: None,
            label: None,
        },
    );
    let _ = fixture.facade.next_host_requests(u16::MAX);

    record_observation(
        &fixture.facade,
        &stream,
        1,
        4_000,
        PlaybackLifecycleObservation {
            episode_id: Some(fixture.episode_id),
            state: PlaybackHostState::Paused,
            position_milliseconds: 120_500,
            duration_milliseconds: 120_500,
            route: PlaybackAudioRoute::BuiltIn,
            interruption: PlaybackInterruption::None,
            ended: true,
        },
    );

    assert_eq!(fixture.playback().current.unwrap().episode_id, replacement);
    assert!(!episode_is_completed(&fixture.facade, replacement));
    assert!(fixture.facade.next_host_requests(u16::MAX).is_empty());
}

#[test]
fn cancelled_playback_stream_rejects_late_position_and_completion() {
    let fixture = PlaybackFixture::new();
    let completed_before = episode_is_completed(&fixture.facade, fixture.episode_id);
    fixture.dispatch(100, PlaybackCommand::Restore);
    let stream = fixture
        .facade
        .next_host_requests(u16::MAX)
        .into_iter()
        .find(|request| matches!(request.request, HostRequest::ObservePlayback { .. }))
        .unwrap();
    fixture.facade.dispatch(CommandEnvelope {
        command_id: CommandId::from_parts(10, 101),
        cancellation_id: CancellationId::from_parts(11, 101),
        expected_revision: None,
        command: ApplicationCommand::CancelOperation {
            cancellation_id: stream.cancellation_id,
        },
    });

    record_observation(
        &fixture.facade,
        &stream,
        1,
        5_000,
        PlaybackLifecycleObservation {
            episode_id: Some(fixture.episode_id),
            state: PlaybackHostState::Paused,
            position_milliseconds: 120_500,
            duration_milliseconds: 120_500,
            route: PlaybackAudioRoute::BuiltIn,
            interruption: PlaybackInterruption::None,
            ended: true,
        },
    );

    let current = fixture.playback().current.unwrap();
    assert_ne!(current.durable_resume_position_milliseconds, 120_500);
    assert_eq!(
        episode_is_completed(&fixture.facade, fixture.episode_id),
        completed_before
    );
}

#[test]
fn end_callback_captured_before_seek_cannot_overwrite_seek_or_complete_episode() {
    let fixture = PlaybackFixture::new();
    fixture.dispatch(110, PlaybackCommand::Restore);
    let stream = fixture
        .facade
        .next_host_requests(u16::MAX)
        .into_iter()
        .find(|request| matches!(request.request, HostRequest::ObservePlayback { .. }))
        .unwrap();
    fixture.dispatch(
        111,
        PlaybackCommand::Seek {
            position_milliseconds: 10_000,
        },
    );
    let _ = fixture.facade.next_host_requests(u16::MAX);

    record_observation(
        &fixture.facade,
        &stream,
        1,
        0,
        PlaybackLifecycleObservation {
            episode_id: Some(fixture.episode_id),
            state: PlaybackHostState::Paused,
            position_milliseconds: 120_500,
            duration_milliseconds: 120_500,
            route: PlaybackAudioRoute::BuiltIn,
            interruption: PlaybackInterruption::None,
            ended: true,
        },
    );

    assert_eq!(
        fixture
            .playback()
            .current
            .unwrap()
            .durable_resume_position_milliseconds,
        10_000
    );
    assert!(!episode_is_completed(&fixture.facade, fixture.episode_id));
}

#[test]
fn late_position_cannot_undo_explicit_completion_until_user_resumes() {
    let fixture = PlaybackFixture::new();
    fixture.dispatch(120, PlaybackCommand::Restore);
    let stream = fixture
        .facade
        .next_host_requests(u16::MAX)
        .into_iter()
        .find(|request| matches!(request.request, HostRequest::ObservePlayback { .. }))
        .unwrap();
    record_observation(
        &fixture.facade,
        &stream,
        1,
        1_000,
        PlaybackLifecycleObservation {
            episode_id: Some(fixture.episode_id),
            state: PlaybackHostState::Playing,
            position_milliseconds: 47_000,
            duration_milliseconds: 120_500,
            route: PlaybackAudioRoute::BuiltIn,
            interruption: PlaybackInterruption::None,
            ended: false,
        },
    );
    fixture.dispatch(
        121,
        PlaybackCommand::SetCompletion {
            episode_id: fixture.episode_id,
            completion: CompletionStatus::Completed {
                cause: CompletionCause::ExplicitUserAction,
            },
        },
    );

    record_observation(
        &fixture.facade,
        &stream,
        2,
        2_000,
        PlaybackLifecycleObservation {
            episode_id: Some(fixture.episode_id),
            state: PlaybackHostState::Playing,
            position_milliseconds: 48_000,
            duration_milliseconds: 120_500,
            route: PlaybackAudioRoute::BuiltIn,
            interruption: PlaybackInterruption::None,
            ended: false,
        },
    );
    assert!(episode_is_completed(&fixture.facade, fixture.episode_id));
    assert_eq!(
        fixture
            .playback()
            .current
            .unwrap()
            .durable_resume_position_milliseconds,
        0
    );

    fixture.dispatch(122, PlaybackCommand::Play);
    let _ = fixture.facade.next_host_requests(u16::MAX);
    record_observation(
        &fixture.facade,
        &stream,
        3,
        3_000,
        PlaybackLifecycleObservation {
            episode_id: Some(fixture.episode_id),
            state: PlaybackHostState::Playing,
            position_milliseconds: 49_000,
            duration_milliseconds: 120_500,
            route: PlaybackAudioRoute::BuiltIn,
            interruption: PlaybackInterruption::None,
            ended: false,
        },
    );
    assert!(!episode_is_completed(&fixture.facade, fixture.episode_id));
    assert_eq!(
        fixture
            .playback()
            .current
            .unwrap()
            .durable_resume_position_milliseconds,
        49_000
    );
}

fn episode_is_completed(facade: &Pod0Facade, episode_id: EpisodeId) -> bool {
    let Projection::Library { value } = facade.snapshot(library_request()).projection else {
        panic!("expected library projection");
    };
    matches!(
        value
            .episodes
            .iter()
            .find(|episode| episode.episode_id == episode_id)
            .unwrap()
            .listening
            .completion,
        CompletionStatus::Completed { .. }
    )
}
