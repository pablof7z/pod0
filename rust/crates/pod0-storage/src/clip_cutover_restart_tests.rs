use std::fs;

use rusqlite::Connection;

use crate::clip_import_tests::{metadata_with_clips, prepare_json_prerequisites};
use crate::listening_import_test_support::*;
use crate::{
    ClipImporter, StorageError, commit_clip_cutover, inspect_legacy_clip_source, read_clip_import,
};

#[test]
fn changed_source_discards_a_stale_stage_and_retries_without_losing_clips() {
    let fixture = ImportFixture::new();
    let original = metadata_with_clips();
    prepare_json_prerequisites(&fixture, &original);
    let original_plan = inspect_legacy_clip_source(&fixture.source).unwrap();
    ClipImporter::new(FixedClock)
        .stage(
            &fixture.source,
            &fixture._directory.path().join("clips-51.backup.json"),
            &fixture.target,
            &fixture.target_backup,
            &original_plan,
            id(5),
            id(4),
        )
        .unwrap();

    let mut changed = original;
    changed["persistenceGeneration"] = serde_json::json!(52);
    changed["clips"][0]["endMs"] = serde_json::json!(15_346);
    fs::write(&fixture.source, serde_json::to_vec(&changed).unwrap()).unwrap();
    assert_eq!(
        commit_clip_cutover(&fixture.source, &fixture.target, 1_721_323_000_101).unwrap_err(),
        StorageError::SourceChanged
    );
    let connection = Connection::open(&fixture.target).unwrap();
    for table in ["pod0_clips", "pod0_clip_state", "pod0_clip_imports"] {
        let count: u32 = connection
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 0, "{table} should be cleared before retry");
    }
    drop(connection);

    let changed_plan = inspect_legacy_clip_source(&fixture.source).unwrap();
    assert_ne!(changed_plan.source_hash, original_plan.source_hash);
    ClipImporter::new(FixedClock)
        .stage(
            &fixture.source,
            &fixture._directory.path().join("clips-52.backup.json"),
            &fixture.target,
            &fixture.target_backup,
            &changed_plan,
            id(6),
            id(4),
        )
        .unwrap();
    assert!(!commit_clip_cutover(&fixture.source, &fixture.target, 1_721_323_000_102).unwrap());
    let snapshot = read_clip_import(&fixture.target, id(6)).unwrap().snapshot;
    assert_eq!(snapshot.clips.len(), 2);
    assert!(
        snapshot
            .clips
            .iter()
            .any(|clip| clip.end_milliseconds == 15_346)
    );
}

#[test]
fn generation_only_source_advance_can_commit_the_verified_clip_snapshot() {
    let fixture = ImportFixture::new();
    let original = metadata_with_clips();
    prepare_json_prerequisites(&fixture, &original);
    let plan = inspect_legacy_clip_source(&fixture.source).unwrap();
    ClipImporter::new(FixedClock)
        .stage(
            &fixture.source,
            &fixture._directory.path().join("clips-51.backup.json"),
            &fixture.target,
            &fixture.target_backup,
            &plan,
            id(5),
            id(4),
        )
        .unwrap();

    let mut advanced = original;
    advanced["persistenceGeneration"] = serde_json::json!(52);
    fs::write(&fixture.source, serde_json::to_vec(&advanced).unwrap()).unwrap();
    let current = inspect_legacy_clip_source(&fixture.source).unwrap();
    assert_eq!(current.source_hash, plan.source_hash);
    assert_ne!(current.source_generation, plan.source_generation);
    assert!(!commit_clip_cutover(&fixture.source, &fixture.target, 1_721_323_000_103).unwrap());
    assert_eq!(
        read_clip_import(&fixture.target, id(5))
            .unwrap()
            .snapshot
            .clips
            .len(),
        2
    );
}

#[test]
fn corrupt_staged_snapshot_cannot_move_the_authoritative_marker() {
    let fixture = ImportFixture::new();
    let original = metadata_with_clips();
    prepare_json_prerequisites(&fixture, &original);
    let plan = inspect_legacy_clip_source(&fixture.source).unwrap();
    ClipImporter::new(FixedClock)
        .stage(
            &fixture.source,
            &fixture._directory.path().join("clips.backup.json"),
            &fixture.target,
            &fixture.target_backup,
            &plan,
            id(5),
            id(4),
        )
        .unwrap();
    let connection = Connection::open(&fixture.target).unwrap();
    connection
        .execute(
            "UPDATE pod0_clips SET frozen_transcript_text='corrupted after staging' \
             WHERE clip_id=(SELECT clip_id FROM pod0_clips LIMIT 1)",
            [],
        )
        .unwrap();
    drop(connection);

    assert!(matches!(
        commit_clip_cutover(&fixture.source, &fixture.target, 1_721_323_000_104),
        Err(StorageError::CorruptSchema { .. })
    ));
    assert_eq!(
        Connection::open(&fixture.target)
            .unwrap()
            .query_row(
                "SELECT state FROM pod0_domain_cutovers WHERE domain='clips'",
                [],
                |row| row.get::<_, String>(0),
            )
            .unwrap(),
        "staged"
    );
}
