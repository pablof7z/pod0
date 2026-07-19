use std::fs;

use pod0_domain::EpisodeId;
use rusqlite::{Connection, params};
use serde_json::Value;

use crate::transcript_import_digest::{digest_bytes, hex_digest};
use crate::transcript_import_test_support::*;
use crate::transcript_legacy_backup::artifact_backup_path;
use crate::{
    StorageError, TranscriptImportState, TranscriptStore, inspect_legacy_transcript_source,
    read_transcript_import,
};

#[test]
fn malformed_missing_mismatched_future_and_duplicate_sources_fail_closed() {
    let missing = TranscriptImportFixture::current();
    fs::remove_file(&missing.selected_path).unwrap();
    assert!(matches!(
        inspect_legacy_transcript_source(&missing.import.source, &missing.transcript_root),
        Err(StorageError::Io { .. })
    ));
    assert_eq!(table_count(&missing, "pod0_transcript_imports"), 0);

    let malformed = TranscriptImportFixture::current();
    malformed.replace_selected_bytes(b"{not-json");
    assert_invalid_source(&malformed);

    let mismatched = TranscriptImportFixture::current();
    mismatched.replace_selected_bytes(&transcript_json(
        "99999999-9999-9999-9999-999999999999",
        "wrong episode",
    ));
    assert_invalid_source(&mismatched);

    let future = TranscriptImportFixture::current();
    Connection::open(&future.import.source)
        .unwrap()
        .execute("UPDATE artifacts SET schema_version=2 WHERE selected=1", [])
        .unwrap();
    assert_eq!(
        inspect_legacy_transcript_source(&future.import.source, &future.transcript_root),
        Err(StorageError::NewerLegacyTranscriptSchema {
            stored: 2,
            supported: 1,
        })
    );

    let duplicate_ids = TranscriptImportFixture::current();
    let mut duplicate_json: Value =
        serde_json::from_slice(&fs::read(&duplicate_ids.selected_path).unwrap()).unwrap();
    duplicate_json["segments"][1]["id"] = duplicate_json["segments"][0]["id"].clone();
    duplicate_ids.replace_selected_bytes(&serde_json::to_vec(&duplicate_json).unwrap());
    assert_invalid_source(&duplicate_ids);

    let duplicate_selection = TranscriptImportFixture::current();
    let selected_bytes = fs::read(&duplicate_selection.selected_path).unwrap();
    Connection::open(&duplicate_selection.import.source)
        .unwrap()
        .execute(
            "INSERT INTO artifacts(kind,subject_id,input_version,output_version,content_hash,\
             location,origin,schema_version,integrity,verified_at,selected) VALUES(\
             'transcript',?1,'input-v2','output-v2',?2,?3,'publisher',1,'available',\
             1900000000,1)",
            params![
                crate::listening_import_test_support::EPISODE_ID,
                hex_digest(digest_bytes(&selected_bytes)),
                duplicate_selection.selected_path.to_string_lossy(),
            ],
        )
        .unwrap();
    assert_invalid_source(&duplicate_selection);
}

#[test]
fn existing_authoritative_cutover_cannot_be_replaced_by_staging() {
    let fixture = TranscriptImportFixture::current();
    Connection::open(&fixture.import.target)
        .unwrap()
        .execute(
            "INSERT INTO pod0_domain_cutovers(domain,state,source_generation,core_revision,\
             committed_at_ms) VALUES('transcripts','authoritative',99,7,1800000000000)",
            [],
        )
        .unwrap();
    assert_eq!(
        fixture.stage(command(32)),
        Err(StorageError::CutoverAlreadyAuthoritative)
    );
    assert_eq!(table_count(&fixture, "pod0_transcript_imports"), 0);
    assert_eq!(table_count(&fixture, "pod0_transcript_selection"), 0);
    assert_eq!(
        Connection::open(&fixture.import.target)
            .unwrap()
            .query_row(
                "SELECT state FROM pod0_domain_cutovers WHERE domain='transcripts'",
                [],
                |row| row.get::<_, String>(0),
            )
            .unwrap(),
        "authoritative"
    );
}

#[test]
fn corrupt_target_or_backup_is_diagnosed_without_selecting_a_transcript() {
    let target_corrupt = TranscriptImportFixture::current();
    target_corrupt.stage(command(30)).unwrap();
    Connection::open(&target_corrupt.import.target)
        .unwrap()
        .execute(
            "UPDATE pod0_transcript_artifact_segments SET raw_text='tampered'",
            [],
        )
        .unwrap();
    assert_eq!(
        target_corrupt.verify(command(30)),
        Err(StorageError::InvalidTranscriptArtifact)
    );
    assert_non_authoritative_corrupt(&target_corrupt, command(30));

    let backup_corrupt = TranscriptImportFixture::current();
    let staged = backup_corrupt.stage(command(31)).unwrap();
    let selected_bytes = fs::read(&backup_corrupt.selected_path).unwrap();
    let path = artifact_backup_path(
        &backup_corrupt.backup_root,
        EpisodeId::from_bytes([0x22; 16]),
        digest_bytes(&selected_bytes),
    );
    fs::write(&path, b"corrupt immutable backup").unwrap();
    assert_eq!(
        backup_corrupt.verify(command(31)),
        Err(StorageError::BackupConflict)
    );
    assert_non_authoritative_corrupt(&backup_corrupt, command(31));
    assert_eq!(
        backup_corrupt.importer.stage(
            &backup_corrupt.import.source,
            &backup_corrupt.transcript_root,
            &backup_corrupt.backup_root,
            &backup_corrupt.import.target,
            &backup_corrupt.import.target_backup,
            &staged.plan,
            command(31),
            command(900),
        ),
        Err(StorageError::BackupConflict)
    );
    assert_eq!(fs::read(path).unwrap(), b"corrupt immutable backup");
}

fn assert_invalid_source(fixture: &TranscriptImportFixture) {
    assert!(matches!(
        inspect_legacy_transcript_source(&fixture.import.source, &fixture.transcript_root),
        Err(StorageError::InvalidLegacyRecord { .. })
    ));
}

fn assert_non_authoritative_corrupt(
    fixture: &TranscriptImportFixture,
    import_id: pod0_domain::CommandId,
) {
    assert_eq!(
        read_transcript_import(&fixture.import.target, import_id)
            .unwrap()
            .state,
        TranscriptImportState::Corrupt
    );
    assert!(
        read_transcript_import(&fixture.import.target, import_id)
            .unwrap()
            .diagnostic_code
            .is_some()
    );
    assert_eq!(
        TranscriptStore::open(&fixture.import.target)
            .unwrap()
            .selected_artifact(EpisodeId::from_bytes([0x22; 16]))
            .unwrap(),
        None
    );
}

fn table_count(fixture: &TranscriptImportFixture, table: &str) -> u32 {
    Connection::open(&fixture.import.target)
        .unwrap()
        .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
            row.get(0)
        })
        .unwrap()
}
