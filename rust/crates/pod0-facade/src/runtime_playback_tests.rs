use crate::runtime_playback_test_support::{
    PlaybackFixture, add_external_episode, library_request, next_playback_requests,
    record_playback,
};
use crate::*;

#[test]
fn restore_observes_once_and_checkpoints_on_first_sample_and_thirty_second_cap() {
    let fixture = PlaybackFixture::new();
    fixture.dispatch(1, PlaybackCommand::Restore);
    let requests = next_playback_requests(&fixture.facade);
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
    fixture.dispatch(
        2,
        PlaybackCommand::Play {
            transcript_configuration: None,
        },
    );
    assert!(next_playback_requests(&fixture.facade).iter().any(|request| {
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
    assert!(next_playback_requests(&fixture.facade).iter().any(|request| {
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
    assert!(next_playback_requests(&fixture.facade).iter().any(|request| {
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
            auto_skip_ads: false,
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
    let stream = next_playback_requests(&fixture.facade)
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
    let effects = next_playback_requests(&fixture.facade);
    assert!(effects.iter().any(|request| {
        matches!(request.request, HostRequest::LoadMedia { episode_id, .. } if episode_id == second)
    }));
    assert!(effects.iter().any(|request| {
        matches!(request.request, HostRequest::Play { episode_id, .. } if episode_id == second)
    }));

    let connection = rusqlite::Connection::open(&fixture.target).unwrap();
    let reaction_transaction: Vec<u8> = connection
        .query_row(
            "SELECT transaction_id FROM pod0_activity_facts WHERE host_request_id=?1 \
             AND fact_code=5 ORDER BY sequence DESC LIMIT 1",
            [stream.request_id.into_bytes().as_slice()],
            |row| row.get(0),
        )
        .unwrap();
    let facts: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM pod0_activity_facts WHERE transaction_id=?1 \
             AND (fact_code=2 OR fact_code=3 OR fact_code=4 OR fact_code=5)",
            [reaction_transaction],
            |row| row.get(0),
        )
        .unwrap();
    assert!(facts >= 4, "one observation, transition, and follow-up effects expected");
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
            auto_skip_ads: false,
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
    let stream = next_playback_requests(&fixture.facade)
        .into_iter()
        .find(|request| matches!(request.request, HostRequest::ObservePlayback { .. }))
        .unwrap();
    fixture.dispatch(35, PlaybackCommand::NativeTimerFired);
    let _ = next_playback_requests(&fixture.facade);

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
