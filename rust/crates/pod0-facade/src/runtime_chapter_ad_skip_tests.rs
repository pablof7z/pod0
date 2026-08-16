use crate::runtime_playback_test_support::{
    PlaybackFixture, PlaybackTestRequest, dispatch, next_playback_requests, playback,
    record_observation,
};
use crate::*;

#[test]
fn automatic_ad_skip_is_coarse_bounded_and_resets_only_with_a_new_session() {
    let fixture = PlaybackFixture::new_with_chapters();
    fixture.dispatch(120, PlaybackCommand::Restore);
    let requests = next_playback_requests(&fixture.facade);
    let stream = requests
        .into_iter()
        .find(|request| matches!(request.request, HostRequest::ObservePlayback { .. }))
        .unwrap();
    let context = fixture.playback().current.unwrap().chapter_context.unwrap();

    record(
        &fixture.facade,
        &stream,
        1,
        1_000,
        0,
        PlaybackHostState::Playing,
    );
    assert!(chapter_seeks(&next_playback_requests(&fixture.facade)).is_empty());
    fixture.dispatch(
        121,
        PlaybackCommand::SetPreferences {
            auto_mark_played_at_natural_end: false,
            auto_play_next: false,
            auto_skip_ads: true,
        },
    );
    assert!(next_playback_requests(&fixture.facade).is_empty());

    record(
        &fixture.facade,
        &stream,
        2,
        2_000,
        0,
        PlaybackHostState::Paused,
    );
    assert!(chapter_seeks(&next_playback_requests(&fixture.facade)).is_empty());
    record(
        &fixture.facade,
        &stream,
        3,
        3_000,
        0,
        PlaybackHostState::Playing,
    );
    let requests = next_playback_requests(&fixture.facade);
    assert!(matches!(
        chapter_seeks(&requests).single().request,
        HostRequest::Seek {
            position_milliseconds: 10_000,
            reason: PlaybackSeekReason::AutomaticAdSkip,
            chapter_context: Some(value),
            ..
        } if value == context
    ));

    record(
        &fixture.facade,
        &stream,
        3,
        3_000,
        0,
        PlaybackHostState::Playing,
    );
    record(
        &fixture.facade,
        &stream,
        4,
        4_000,
        1_000,
        PlaybackHostState::Playing,
    );
    assert!(chapter_seeks(&next_playback_requests(&fixture.facade)).is_empty());
    fixture.dispatch(
        122,
        PlaybackCommand::Seek {
            position_milliseconds: 1_000,
        },
    );
    let manual = next_playback_requests(&fixture.facade);
    assert!(manual.iter().any(|request| matches!(
        request.request,
        HostRequest::Seek {
            reason: PlaybackSeekReason::UserRequested,
            chapter_context: None,
            ..
        }
    )));
    record(
        &fixture.facade,
        &stream,
        5,
        5_000,
        1_000,
        PlaybackHostState::Playing,
    );
    assert!(chapter_seeks(&next_playback_requests(&fixture.facade)).is_empty());

    let reopened = Pod0Facade::open(fixture.target.to_string_lossy().into_owned()).unwrap();
    dispatch(&reopened, 130, PlaybackCommand::Restore);
    let requests = next_playback_requests(&reopened);
    let reopened_stream = requests
        .into_iter()
        .find(|request| matches!(request.request, HostRequest::ObservePlayback { .. }))
        .unwrap();
    dispatch(
        &reopened,
        131,
        PlaybackCommand::SetPreferences {
            auto_mark_played_at_natural_end: false,
            auto_play_next: false,
            auto_skip_ads: true,
        },
    );
    let reopened_context = playback(&reopened)
        .current
        .unwrap()
        .chapter_context
        .unwrap();
    assert_ne!(reopened_context.session_id, context.session_id);
    record(&reopened, &stream, 6, 5_500, 0, PlaybackHostState::Playing);
    assert!(
        chapter_seeks(&next_playback_requests(&reopened)).is_empty(),
        "an observation stream from the prior playback session must be rejected"
    );
    record(
        &reopened,
        &reopened_stream,
        1,
        6_000,
        0,
        PlaybackHostState::Playing,
    );
    assert_eq!(chapter_seeks(&next_playback_requests(&reopened)).len(), 1);
}

fn record(
    facade: &Pod0Facade,
    stream: &PlaybackTestRequest,
    sequence: u64,
    observed_at: i64,
    position: u64,
    state: PlaybackHostState,
) {
    record_observation(
        facade,
        stream,
        sequence,
        observed_at,
        PlaybackLifecycleObservation {
            episode_id: playback(facade).current.map(|item| item.episode_id),
            state,
            position_milliseconds: position,
            duration_milliseconds: 120_500,
            route: PlaybackAudioRoute::BuiltIn,
            interruption: PlaybackInterruption::None,
            ended: false,
        },
    );
}

fn chapter_seeks(requests: &[PlaybackTestRequest]) -> Vec<&PlaybackTestRequest> {
    requests
        .iter()
        .filter(|request| {
            matches!(
                request.request,
                HostRequest::Seek {
                    chapter_context: Some(_),
                    ..
                }
            )
        })
        .collect()
}

trait Single<T> {
    fn single(&self) -> T;
}

impl<T: Copy> Single<T> for Vec<T> {
    fn single(&self) -> T {
        assert_eq!(self.len(), 1);
        self[0]
    }
}
