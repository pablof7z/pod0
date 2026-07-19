use pod0_application::{TranscriptCommitRequest, qualify_transcript_commit};
use pod0_domain::{StateRevision, TranscriptArtifact};
use rusqlite::Connection;

use crate::transcript_store_test_support::*;
use crate::{StorageError, TranscriptStore};

#[test]
fn commit_reopens_losslessly_and_all_projections_are_bounded() {
    let fixture = TranscriptFixture::new();
    let input = input("transcript-v1");
    let expected_artifact = TranscriptArtifact::seal(input.clone()).unwrap();
    let expected_contract = qualify_transcript_commit(TranscriptCommitRequest {
        command_id: command(10),
        expected_selection_revision: StateRevision::INITIAL,
        artifact: input.clone(),
    })
    .unwrap();

    let receipt = fixture
        .store
        .commit_and_select(
            command(10),
            StateRevision::INITIAL,
            input,
            1_800_000_000_010,
        )
        .unwrap();

    assert_eq!(receipt.artifact_id, expected_contract.artifact_id);
    assert_eq!(
        receipt.command_fingerprint,
        expected_contract.command_fingerprint
    );
    assert_eq!(receipt.selection_revision, StateRevision::new(1));
    assert_eq!(receipt.word_count, 3);
    assert!(!receipt.already_selected);
    assert_eq!(
        fixture
            .store
            .selected_artifact(expected_artifact.episode_id)
            .unwrap(),
        Some(expected_artifact.clone())
    );
    let summary = fixture
        .store
        .selected_summary(expected_artifact.episode_id)
        .unwrap()
        .unwrap();
    assert_eq!(summary.artifact_id, expected_artifact.artifact_id);
    assert_eq!(summary.speaker_count, 2);
    assert_eq!(summary.segment_count, 2);
    assert_eq!(summary.word_count, 3);

    let speakers = fixture
        .store
        .selected_speakers(expected_artifact.episode_id, 0, 1)
        .unwrap();
    assert_eq!(speakers.items[0].display_name.as_deref(), Some("Ada"));
    assert!(speakers.has_more);
    let segments = fixture
        .store
        .selected_segments(expected_artifact.episode_id, 0, 1)
        .unwrap();
    assert_eq!(segments.items[0].text, "Small   habits become durable");
    assert!(segments.has_more);
    let words = fixture
        .store
        .selected_words(
            expected_artifact.episode_id,
            expected_artifact.segments[0].segment_id,
            0,
            1,
        )
        .unwrap();
    assert_eq!(words.items[0].text, "Small");
    assert!(words.has_more);
    assert_eq!(
        fixture
            .store
            .selected_segment(
                expected_artifact.episode_id,
                expected_artifact.segments[1].segment_id
            )
            .unwrap()
            .unwrap()
            .word_count,
        1
    );

    let reopened = TranscriptStore::open(&fixture.import.target).unwrap();
    assert_eq!(
        reopened
            .selected_artifact(expected_artifact.episode_id)
            .unwrap(),
        Some(expected_artifact)
    );
    let normalized: String = Connection::open(&fixture.import.target)
        .unwrap()
        .query_row(
            "SELECT text FROM pod0_transcript_segments WHERE ordinal=0",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(normalized, "Small habits become durable");
}

#[test]
fn command_replay_is_stable_after_later_selection_and_conflicts_fail_closed() {
    let fixture = TranscriptFixture::new();
    let first_input = input("transcript-v1");
    let first = fixture
        .store
        .commit_and_select(
            command(20),
            StateRevision::INITIAL,
            first_input.clone(),
            1_800_000_000_020,
        )
        .unwrap();
    let second = fixture
        .store
        .commit_and_select(
            command(21),
            StateRevision::new(1),
            input("transcript-v2"),
            1_800_000_000_021,
        )
        .unwrap();
    assert_eq!(second.previous_artifact_id, Some(first.artifact_id));
    assert_eq!(second.selection_revision, StateRevision::new(2));
    assert_eq!(
        fixture
            .store
            .commit_and_select(
                command(20),
                StateRevision::INITIAL,
                first_input,
                1_900_000_000_000
            )
            .unwrap(),
        first
    );

    assert_eq!(
        fixture.store.commit_and_select(
            command(20),
            StateRevision::new(2),
            input("conflict"),
            1_800_000_000_022
        ),
        Err(StorageError::TranscriptCommandConflict)
    );
    assert_eq!(
        fixture.store.commit_and_select(
            command(22),
            StateRevision::INITIAL,
            input("stale"),
            1_800_000_000_022
        ),
        Err(StorageError::TranscriptRevisionConflict)
    );
    assert_eq!(
        fixture
            .store
            .selected_summary(second_artifact_episode())
            .unwrap()
            .unwrap()
            .selection_revision,
        StateRevision::new(2)
    );
}

fn second_artifact_episode() -> pod0_domain::EpisodeId {
    input("ignored").episode_id
}
