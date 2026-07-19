use pod0_domain::{StateRevision, TranscriptArtifact};
use rusqlite::Connection;

use crate::transcript_store_test_support::*;
use crate::{EvidenceStore, StorageError, TranscriptStore};

#[test]
fn interruption_rolls_back_all_rows_and_restart_can_replay_cleanly() {
    let fixture = TranscriptFixture::new();
    let source_input = input("interrupted");
    assert_eq!(
        fixture.store.commit_and_select_with_observer(
            command(30),
            StateRevision::INITIAL,
            source_input.clone(),
            1_800_000_000_030,
            || Err(StorageError::Interrupted),
        ),
        Err(StorageError::Interrupted)
    );
    let connection = Connection::open(&fixture.import.target).unwrap();
    for table in [
        "pod0_transcript_artifacts",
        "pod0_transcript_selection",
        "pod0_transcript_commands",
    ] {
        assert_eq!(count(&connection, table), 0, "{table}");
    }
    assert_eq!(count(&connection, "pod0_transcript_documents"), 0);
    drop(connection);

    let reopened = TranscriptStore::open(&fixture.import.target).unwrap();
    let receipt = reopened
        .commit_and_select(
            command(30),
            StateRevision::INITIAL,
            source_input.clone(),
            1_800_000_000_031,
        )
        .unwrap();
    assert_eq!(
        reopened.selected_artifact(source_input.episode_id).unwrap(),
        Some(TranscriptArtifact::seal(source_input).unwrap())
    );
    assert_eq!(receipt.selection_revision, StateRevision::new(1));
}

#[test]
fn transcript_and_evidence_share_semantic_rows_without_mutating_frozen_evidence() {
    let fixture = TranscriptFixture::new();
    let source_input = input("shared-version");
    let evidence = evidence(&source_input);
    let evidence_store = EvidenceStore::open(&fixture.import.target).unwrap();
    evidence_store
        .stage_artifact(command(40), &evidence, 1_800_000_000_040)
        .unwrap();
    evidence_store
        .verify_generation(command(41), evidence.generation_id, 1_800_000_000_041)
        .unwrap();

    fixture
        .store
        .commit_and_select(
            command(42),
            StateRevision::INITIAL,
            source_input,
            1_800_000_000_042,
        )
        .unwrap();
    let connection = Connection::open(&fixture.import.target).unwrap();
    assert_eq!(count(&connection, "pod0_transcript_documents"), 1);
    assert_eq!(count(&connection, "pod0_transcript_segments"), 2);
    drop(connection);
    let frozen_version = evidence.version.transcript_version_id;

    fixture
        .store
        .commit_and_select(
            command(43),
            StateRevision::new(1),
            input("replacement"),
            1_800_000_000_043,
        )
        .unwrap();
    assert_eq!(
        evidence_store
            .generation(evidence.generation_id)
            .unwrap()
            .unwrap()
            .version
            .transcript_version_id,
        frozen_version
    );
    assert!(
        evidence_store
            .prune_unselected_generation(command(44), evidence.generation_id, 1_800_000_000_044)
            .unwrap()
            .pruned
    );
    let connection = Connection::open(&fixture.import.target).unwrap();
    assert_eq!(count(&connection, "pod0_transcript_documents"), 2);
}

fn count(connection: &Connection, table: &str) -> i64 {
    connection
        .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
            row.get(0)
        })
        .unwrap()
}
