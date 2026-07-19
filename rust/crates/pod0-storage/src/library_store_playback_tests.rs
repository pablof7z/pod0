use pod0_domain::{
    CompletionCause, CompletionStatus, PlaybackSegment, PlaybackSleepMode, QueueEntry, QueueEntryId,
};

use crate::listening_import_test_support::*;
use crate::{LibraryStore, PlaybackMutation, PlaybackQueuePlacement, commit_listening_cutover};

#[test]
fn playback_slice_commits_selection_queue_resume_and_natural_advance_atomically() {
    let fixture = imported_fixture();
    commit_listening_cutover(&fixture.target, 1_800_000_000_000).unwrap();
    let store = LibraryStore::open_authoritative(&fixture.target).unwrap();
    let initial = store.snapshot().unwrap();
    let first = initial.episodes[0].episode_id;
    let podcast_id = initial.podcasts[0].podcast_id;
    let (_, _, second) = store
        .upsert_external_episode(
            id(20),
            &"e".repeat(64),
            podcast_id,
            None,
            "Imported show",
            "https://example.test/second.mp3",
            "Second episode",
            None,
            Some(600_000),
            1_800_000_000_001,
        )
        .unwrap();
    let segment = PlaybackSegment {
        start_position_milliseconds: Some(10_000),
        end_position_milliseconds: Some(20_000),
    };

    store
        .apply_playback_mutation(
            id(21),
            &"s".repeat(64),
            PlaybackMutation::Select {
                episode_id: first,
                segment: Some(segment),
                label: Some("Evidence".to_owned()),
            },
            1_800_000_000_002,
        )
        .unwrap();
    store
        .apply_playback_mutation(
            id(23),
            &"p".repeat(64),
            PlaybackMutation::SetPreferences {
                auto_mark_played_at_natural_end: true,
                auto_play_next: true,
            },
            1_800_000_000_002,
        )
        .unwrap();
    let queue_entry = QueueEntry {
        queue_entry_id: QueueEntryId::from_parts(0, 1),
        episode_id: second,
        segment: None,
        label: None,
    };
    let queued = store
        .apply_playback_mutation(
            id(22),
            &"q".repeat(64),
            PlaybackMutation::Enqueue {
                entry: queue_entry.clone(),
                placement: PlaybackQueuePlacement::Back,
            },
            1_800_000_000_003,
        )
        .unwrap();
    assert_eq!(
        store
            .apply_playback_mutation(
                id(22),
                &"q".repeat(64),
                PlaybackMutation::Enqueue {
                    entry: queue_entry,
                    placement: PlaybackQueuePlacement::Back,
                },
                1_800_000_000_004,
            )
            .unwrap(),
        queued
    );
    store
        .apply_playback_observation(
            PlaybackMutation::Checkpoint {
                episode_id: first,
                position_milliseconds: 19_500,
            },
            1_800_000_000_005,
        )
        .unwrap();
    store
        .apply_playback_observation(
            PlaybackMutation::FinishActive {
                suppress_auto_advance: false,
            },
            1_800_000_000_006,
        )
        .unwrap();

    let after = store.snapshot().unwrap();
    assert_eq!(after.playback.active_episode_id, Some(second));
    assert!(after.playback.queue.is_empty());
    assert_eq!(after.playback.active_segment, None);
    let completed = after
        .episodes
        .iter()
        .find(|episode| episode.episode_id == first)
        .unwrap();
    assert_eq!(completed.listening.resume_position_milliseconds, 0);
    assert_eq!(
        completed.listening.completion,
        CompletionStatus::Completed {
            cause: CompletionCause::NaturalEnd
        }
    );
}

#[test]
fn session_sleep_timer_is_cleared_once_without_erasing_other_playback_state() {
    let fixture = imported_fixture();
    commit_listening_cutover(&fixture.target, 1_800_000_000_000).unwrap();
    let store = LibraryStore::open_authoritative(&fixture.target).unwrap();
    let episode_id = store.snapshot().unwrap().episodes[0].episode_id;
    store
        .apply_playback_mutation(
            id(30),
            &"t".repeat(64),
            PlaybackMutation::Select {
                episode_id,
                segment: None,
                label: None,
            },
            1_800_000_000_001,
        )
        .unwrap();
    store
        .apply_playback_mutation(
            id(31),
            &"u".repeat(64),
            PlaybackMutation::SetSleepTimer(PlaybackSleepMode::Duration {
                duration_milliseconds: 900_000,
            }),
            1_800_000_000_002,
        )
        .unwrap();

    let cleared = store.clear_session_sleep_timer().unwrap();
    let after = store.snapshot().unwrap();
    assert_eq!(after.playback.sleep_mode, PlaybackSleepMode::Off);
    assert_eq!(after.playback.active_episode_id, Some(episode_id));
    assert_eq!(store.clear_session_sleep_timer().unwrap(), cleared);
}

fn imported_fixture() -> ImportFixture {
    let fixture = ImportFixture::new();
    create_sqlite_source(
        &fixture.source,
        &current_metadata(7),
        &[episode(EPISODE_ID, "guid-1")],
    );
    fixture.stage(&fixture.plan()).unwrap();
    fixture
}
