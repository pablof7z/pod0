use rusqlite::Connection;

use crate::transcript_import_test_support::*;
use crate::transcript_legacy_backup::database_backup_path;
use crate::transcript_store_test_support::input as replacement_input;
use crate::{StorageError, TranscriptImportState, TranscriptStore, read_transcript_import};

#[test]
fn stale_target_revision_is_discarded_and_can_be_restaged_safely() {
    let fixture = TranscriptImportFixture::current();
    fixture.stage(command(46)).unwrap();
    fixture.verify(command(46)).unwrap();
    let store = TranscriptStore::open(&fixture.import.target).unwrap();
    let replacement = seed_pre_authority_selection(
        &fixture.import.target,
        replacement_input("concurrent-shared-selection"),
    );

    assert_eq!(
        fixture.commit(command(46)),
        Err(StorageError::TranscriptImportConflict)
    );
    let discarded = read_transcript_import(&fixture.import.target, command(46)).unwrap();
    assert_eq!(discarded.state, TranscriptImportState::Discarded);
    assert_eq!(
        discarded.diagnostic_code.as_deref(),
        Some(StorageError::TranscriptImportConflict.code())
    );
    assert_eq!(
        store
            .selected_summary(pod0_domain::EpisodeId::from_bytes([0x22; 16]))
            .unwrap()
            .unwrap()
            .artifact_id,
        replacement.artifact_id
    );
    assert_eq!(
        fixture.stage(command(48)).unwrap().state,
        TranscriptImportState::Staged
    );
}

#[test]
fn authoritative_cutover_rejects_a_later_legacy_swift_selection() {
    let fixture = TranscriptImportFixture::current();
    let first_plan = fixture.plan();
    fixture.stage(command(49)).unwrap();
    fixture.verify(command(49)).unwrap();
    fixture.commit(command(49)).unwrap();
    let store = TranscriptStore::open(&fixture.import.target).unwrap();
    let episode_id = pod0_domain::EpisodeId::from_bytes([0x22; 16]);
    let first = store.selected_artifact(episode_id).unwrap().unwrap();

    fixture.replace_selected("a newer Swift-selected transcript");
    let second_plan = fixture.plan();
    assert_ne!(
        first_plan.source_selection_digest,
        second_plan.source_selection_digest
    );
    assert_eq!(
        fixture.stage(command(50)),
        Err(StorageError::CutoverAlreadyAuthoritative)
    );
    let second = store.selected_artifact(episode_id).unwrap().unwrap();
    assert_eq!(second.artifact_id, first.artifact_id);
    assert_ne!(second.segments[0].text, "a newer Swift-selected transcript");
    assert_eq!(
        store
            .selected_summary(episode_id)
            .unwrap()
            .unwrap()
            .selection_revision
            .value,
        1
    );
    assert_eq!(
        Connection::open(&fixture.import.target)
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM pod0_transcript_artifacts",
                [],
                |row| { row.get::<_, u32>(0) }
            )
            .unwrap(),
        1
    );
    assert_eq!(
        read_transcript_import(&fixture.import.target, command(49))
            .unwrap()
            .state,
        TranscriptImportState::Committed
    );
    let first_backup = database_backup_path(&fixture.backup_root, &first_plan);
    let second_backup = database_backup_path(&fixture.backup_root, &second_plan);
    assert_ne!(first_backup, second_backup);
    assert!(first_backup.exists());
    assert!(!second_backup.exists());
}
