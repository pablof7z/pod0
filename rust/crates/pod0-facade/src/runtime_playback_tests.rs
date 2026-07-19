use crate::runtime_playback_test_support::{
    PlaybackFixture, add_external_episode, library_request, record_playback,
};
use crate::*;

#[test]
fn restore_observes_once_and_checkpoints_on_first_sample_and_thirty_second_cap() {
    let fixture = PlaybackFixture::new();
    fixture.dispatch(1, PlaybackCommand::Restore);
    let requests = fixture.facade.next_host_requests(u16::MAX);
    assert_eq!(
        requests
            .iter()
            .filter(|request| matches!(request.request, HostRequest::ObservePlayback { .. }))
            .count(),
        1
    );
    assert!(requests.iter().any(|request| matches!(
        request.request,
        HostRequest::LoadMedia { episode_id, .. } if episode_id == fixture.episode_id
    )));
    let stream = requests
        .into_iter()
        .find(|request| matches!(request.request, HostRequest::ObservePlayback { .. }))
        .unwrap();
    fixture.dispatch(2, PlaybackCommand::Play);
    assert!(fixture.facade.next_host_requests(u16::MAX).iter().any(|request| {
        matches!(request.request, HostRequest::Play { episode_id, .. } if episode_id == fixture.episode_id)
    }));

    record_playback(
        &fixture.facade,
        &stream,
        1,
        1_000,
        40_000,
        false,
        PlaybackInterruption::None,
    );
    assert_eq!(
        fixture
            .playback()
            .current
            .unwrap()
            .durable_resume_position_milliseconds,
        40_000
    );
    record_playback(
        &fixture.facade,
        &stream,
        2,
        5_000,
        41_000,
        false,
        PlaybackInterruption::None,
    );
    assert_eq!(
        fixture
            .playback()
            .current
            .unwrap()
            .durable_resume_position_milliseconds,
        40_000
    );
    record_playback(
        &fixture.facade,
        &stream,
        3,
        31_001,
        42_000,
        false,
        PlaybackInterruption::None,
    );
    assert_eq!(
        fixture
            .playback()
            .current
            .unwrap()
            .durable_resume_position_milliseconds,
        42_000
    );

    record_playback(
        &fixture.facade,
        &stream,
        4,
        32_000,
        42_500,
        false,
        PlaybackInterruption::Began,
    );
    assert!(fixture.facade.next_host_requests(u16::MAX).iter().any(|request| {
        matches!(request.request, HostRequest::Pause { episode_id } if episode_id == fixture.episode_id)
    }));
    record_playback(
        &fixture.facade,
        &stream,
        5,
        33_000,
        42_500,
        false,
        PlaybackInterruption::EndedShouldResume,
    );
    assert!(fixture.facade.next_host_requests(u16::MAX).iter().any(|request| {
        matches!(request.request, HostRequest::Play { episode_id, .. } if episode_id == fixture.episode_id)
    }));
}

#[test]
fn natural_end_completes_and_advances_the_queue_through_one_rust_transaction() {
    let fixture = PlaybackFixture::new();
    let second = add_external_episode(&fixture, 20);
    fixture.dispatch(
        21,
        PlaybackCommand::SetPreferences {
            auto_mark_played_at_natural_end: true,
            auto_play_next: true,
        },
    );
    fixture.dispatch(
        22,
        PlaybackCommand::Select {
            episode_id: fixture.episode_id,
            segment: None,
            label: None,
        },
    );
    fixture.dispatch(
        23,
        PlaybackCommand::Enqueue {
            entry: QueueEntry {
                queue_entry_id: QueueEntryId::from_parts(12, 1),
                episode_id: second,
                segment: None,
                label: None,
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
        40_000,
        120_500,
        true,
        PlaybackInterruption::None,
    );

    let playback = fixture.playback();
    assert_eq!(playback.current.unwrap().episode_id, second);
    assert!(playback.queue.is_empty());
    let Projection::Library { value } = fixture.facade.snapshot(library_request()).projection
    else {
        panic!("expected library projection");
    };
    assert!(matches!(
        value
            .episodes
            .iter()
            .find(|episode| episode.episode_id == fixture.episode_id)
            .unwrap()
            .listening
            .completion,
        CompletionStatus::Completed {
            cause: CompletionCause::NaturalEnd
        }
    ));
    let effects = fixture.facade.next_host_requests(u16::MAX);
    assert!(effects.iter().any(|request| {
        matches!(request.request, HostRequest::LoadMedia { episode_id, .. } if episode_id == second)
    }));
    assert!(effects.iter().any(|request| {
        matches!(request.request, HostRequest::Play { episode_id, .. } if episode_id == second)
    }));
}

#[test]
fn fired_sleep_timer_suppresses_autoplay_even_when_preferences_allow_it() {
    let fixture = PlaybackFixture::new();
    let second = add_external_episode(&fixture, 30);
    fixture.dispatch(
        31,
        PlaybackCommand::SetPreferences {
            auto_mark_played_at_natural_end: true,
            auto_play_next: true,
        },
    );
    fixture.dispatch(
        32,
        PlaybackCommand::Select {
            episode_id: fixture.episode_id,
            segment: None,
            label: None,
        },
    );
    fixture.dispatch(
        33,
        PlaybackCommand::Enqueue {
            entry: QueueEntry {
                queue_entry_id: QueueEntryId::from_parts(12, 2),
                episode_id: second,
                segment: None,
                label: None,
            },
            placement: QueuePlacement::Back,
        },
    );
    fixture.dispatch(
        34,
        PlaybackCommand::SetSleepTimer {
            mode: PlaybackSleepMode::EndOfEpisode,
        },
    );
    let stream = fixture
        .facade
        .next_host_requests(u16::MAX)
        .into_iter()
        .find(|request| matches!(request.request, HostRequest::ObservePlayback { .. }))
        .unwrap();
    fixture.dispatch(35, PlaybackCommand::NativeTimerFired);
    let _ = fixture.facade.next_host_requests(u16::MAX);

    record_playback(
        &fixture.facade,
        &stream,
        1,
        50_000,
        120_500,
        true,
        PlaybackInterruption::None,
    );

    let playback = fixture.playback();
    assert_eq!(playback.current.unwrap().episode_id, fixture.episode_id);
    assert_eq!(playback.queue[0].episode_id, second);
    assert_eq!(playback.sleep_mode, PlaybackSleepMode::Off);
}
