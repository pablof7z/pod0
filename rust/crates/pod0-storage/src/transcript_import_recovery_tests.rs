use rusqlite::Connection;

use crate::transcript_import_commit::commit_transcript_import_with_observer;
use crate::transcript_import_test_support::*;
use crate::{
    StorageError, TranscriptImportState, TranscriptImporter, TranscriptStore,
    read_transcript_import,
};

#[test]
fn interrupted_stage_rolls_back_target_and_retry_reuses_verified_backups() {
    let fixture = TranscriptImportFixture::current();
    let plan = fixture.plan();
    assert_eq!(
        fixture.importer.stage_with_observer(
            &fixture.import.source,
            &fixture.transcript_root,
            &fixture.backup_root,
            &fixture.import.target,
            &fixture.import.target_backup,
            &plan,
            command(40),
            command(900),
            || Err(StorageError::Interrupted),
        ),
        Err(StorageError::Interrupted)
    );
    let connection = Connection::open(&fixture.import.target).unwrap();
    for table in [
        "pod0_transcript_imports",
        "pod0_transcript_import_entries",
        "pod0_transcript_artifacts",
        "pod0_transcript_selection",
    ] {
        assert_eq!(count(&connection, table), 0, "{table} should roll back");
    }
    assert_eq!(
        connection
            .query_row(
                "SELECT COUNT(*) FROM pod0_domain_cutovers WHERE domain='transcripts'",
                [],
                |row| row.get::<_, u32>(0),
            )
            .unwrap(),
        0
    );
    drop(connection);

    let recovered = fixture.importer.stage(
        &fixture.import.source,
        &fixture.transcript_root,
        &fixture.backup_root,
        &fixture.import.target,
        &fixture.import.target_backup,
        &plan,
        command(40),
        command(900),
    );
    let recovered = recovered.unwrap();
    assert!(recovered.backup.reused_database);
    assert_eq!(recovered.backup.reused_artifacts, 1);
}

#[test]
fn independent_importer_instances_resume_staged_verified_and_committed_states() {
    let fixture = TranscriptImportFixture::current();
    fixture.stage(command(41)).unwrap();

    let after_stage = TranscriptImporter::new(FixedTranscriptClock)
        .verify(&fixture.import.target, &fixture.backup_root, command(41))
        .unwrap();
    assert_eq!(after_stage.report.state, TranscriptImportState::Verified);

    let after_verify = TranscriptImporter::new(FixedTranscriptClock)
        .commit(
            &fixture.import.source,
            &fixture.transcript_root,
            &fixture.import.target,
            command(41),
        )
        .unwrap();
    assert_eq!(after_verify.state, TranscriptImportState::Committed);

    let after_commit = TranscriptImporter::new(FixedTranscriptClock)
        .commit(
            &fixture.import.source,
            &fixture.transcript_root,
            &fixture.import.target,
            command(41),
        )
        .unwrap();
    assert_eq!(after_commit.state, TranscriptImportState::Committed);
    assert!(after_commit.reused_existing);
}

#[test]
fn source_change_before_or_during_commit_discards_without_partial_selection() {
    let before = TranscriptImportFixture::current();
    before.stage(command(42)).unwrap();
    before.verify(command(42)).unwrap();
    before.replace_selected("source changed before commit");
    assert_eq!(before.commit(command(42)), Err(StorageError::SourceChanged));
    assert_discarded_without_selection(&before, command(42));

    let during = TranscriptImportFixture::current();
    during.stage(command(43)).unwrap();
    during.verify(command(43)).unwrap();
    assert_eq!(
        commit_transcript_import_with_observer(
            &during.import.source,
            &during.transcript_root,
            &during.import.target,
            command(43),
            1_800_000_000_001,
            || {
                during.replace_selected("source changed inside commit fence");
                Ok(())
            },
        ),
        Err(StorageError::SourceChanged)
    );
    assert_discarded_without_selection(&during, command(43));
}

#[test]
fn source_change_inside_stage_fence_rolls_back_every_target_row() {
    let fixture = TranscriptImportFixture::current();
    let plan = fixture.plan();
    assert_eq!(
        fixture.importer.stage_with_observer(
            &fixture.import.source,
            &fixture.transcript_root,
            &fixture.backup_root,
            &fixture.import.target,
            &fixture.import.target_backup,
            &plan,
            command(44),
            command(900),
            || {
                fixture.replace_selected("changed inside stage fence");
                Ok(())
            },
        ),
        Err(StorageError::SourceChanged)
    );
    let connection = Connection::open(&fixture.import.target).unwrap();
    for table in [
        "pod0_transcript_imports",
        "pod0_transcript_import_entries",
        "pod0_transcript_artifacts",
        "pod0_transcript_selection",
    ] {
        assert_eq!(count(&connection, table), 0, "{table} should roll back");
    }
}

#[test]
fn interrupted_commit_stays_verified_and_retry_commits_once() {
    let fixture = TranscriptImportFixture::current();
    fixture.stage(command(45)).unwrap();
    fixture.verify(command(45)).unwrap();
    assert_eq!(
        commit_transcript_import_with_observer(
            &fixture.import.source,
            &fixture.transcript_root,
            &fixture.import.target,
            command(45),
            1_800_000_000_001,
            || Err(StorageError::Interrupted),
        ),
        Err(StorageError::Interrupted)
    );
    assert_eq!(
        read_transcript_import(&fixture.import.target, command(45))
            .unwrap()
            .state,
        TranscriptImportState::Verified
    );
    assert!(
        TranscriptStore::open(&fixture.import.target)
            .unwrap()
            .selected_artifact(pod0_domain::EpisodeId::from_bytes([0x22; 16]))
            .unwrap()
            .is_none()
    );
    assert_eq!(
        fixture.commit(command(45)).unwrap().state,
        TranscriptImportState::Committed
    );
}

fn assert_discarded_without_selection(
    fixture: &TranscriptImportFixture,
    import_id: pod0_domain::CommandId,
) {
    let report = read_transcript_import(&fixture.import.target, import_id).unwrap();
    assert_eq!(report.state, TranscriptImportState::Discarded);
    assert_eq!(
        report.diagnostic_code.as_deref(),
        Some(StorageError::SourceChanged.code())
    );
    assert_eq!(
        TranscriptStore::open(&fixture.import.target)
            .unwrap()
            .selected_artifact(pod0_domain::EpisodeId::from_bytes([0x22; 16]))
            .unwrap(),
        None
    );
    assert_eq!(
        Connection::open(&fixture.import.target)
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM pod0_domain_cutovers WHERE domain='transcripts'",
                [],
                |row| row.get::<_, u32>(0),
            )
            .unwrap(),
        0
    );
}

fn count(connection: &Connection, table: &str) -> u32 {
    connection
        .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
            row.get(0)
        })
        .unwrap()
}
