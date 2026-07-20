use std::fs;

use crate::chapter_import_source_tests::{
    episode_with_chapters, episode_without_adjunct, workflow_chapters,
};
use crate::chapter_import_test_support::{ChapterImportFixture, EPISODE_ID, PODCAST_ID};
use crate::chapter_rollback_export::export_chapter_rollback_bundle_with_observer;
use crate::{StorageError, export_chapter_rollback_bundle, inspect_legacy_chapter_source};

#[test]
fn imported_history_exports_as_an_idempotent_versioned_bundle() {
    let fixture = imported_fixture();
    let first = export_chapter_rollback_bundle(
        &fixture.target,
        &fixture.legacy_backup,
        &fixture.rollback_root,
    )
    .unwrap();
    assert_eq!(first.format_version, 1);
    assert_eq!(first.evidence_count, 1);
    assert_eq!(first.artifact_count, 1);
    assert!(!first.reused_existing);
    assert!(first.bundle_path.join("manifest.json").is_file());
    assert!(first.bundle_path.join("source.sqlite").is_file());
    assert!(first.bundle_path.join("original-source.sqlite").is_file());
    assert!(first.bundle_path.join("bundle.digest").is_file());

    let replay = export_chapter_rollback_bundle(
        &fixture.target,
        &fixture.legacy_backup,
        &fixture.rollback_root,
    )
    .unwrap();
    assert!(replay.reused_existing);
    assert_eq!(replay.bundle_path, first.bundle_path);
    assert_eq!(replay.bundle_digest, first.bundle_digest);
}

#[test]
fn workflow_rollback_database_replays_from_bundle_relative_evidence() {
    let fixture = ChapterImportFixture::new_v1();
    fixture.insert_episode(EPISODE_ID, PODCAST_ID, &episode_without_adjunct(EPISODE_ID));
    fixture.insert_workflow_artifact(
        "chapters",
        EPISODE_ID,
        "workflow-input",
        "workflow-output",
        "generated",
        "available",
        1_800_000_210.0,
        true,
        &workflow_chapters("Replayable", true),
    );
    fixture.stage(1_800_000_210_000);
    fixture.verify(1_800_000_210_001);
    fixture.import(1_800_000_210_002);
    let report = export_chapter_rollback_bundle(
        &fixture.target,
        &fixture.legacy_backup,
        &fixture.rollback_root,
    )
    .unwrap();

    let replay = inspect_legacy_chapter_source(
        &report.bundle_path.join("source.sqlite"),
        &report.bundle_path,
    )
    .unwrap();
    assert_eq!(replay.source_generation, 7);
    assert_eq!(replay.canonical_artifact_count, 1);
    assert_eq!(replay.selected_count, 1);
    assert_eq!(replay.blocked_count, 0);
}

#[test]
fn tampered_existing_rollback_bundle_fails_closed() {
    let fixture = imported_fixture();
    let report = export_chapter_rollback_bundle(
        &fixture.target,
        &fixture.legacy_backup,
        &fixture.rollback_root,
    )
    .unwrap();
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(report.bundle_path.join("manifest.json")).unwrap())
            .unwrap();
    let relative = manifest["entries"][0]["relative_path"].as_str().unwrap();
    fs::write(report.bundle_path.join(relative), b"tampered").unwrap();

    assert!(matches!(
        export_chapter_rollback_bundle(
            &fixture.target,
            &fixture.legacy_backup,
            &fixture.rollback_root,
        ),
        Err(StorageError::BackupConflict)
    ));
}

#[test]
fn interrupted_rollback_publish_leaves_no_partial_bundle_and_retries() {
    let fixture = imported_fixture();
    let result = export_chapter_rollback_bundle_with_observer(
        &fixture.target,
        &fixture.legacy_backup,
        &fixture.rollback_root,
        || Err(StorageError::Interrupted),
    );
    assert!(matches!(result, Err(StorageError::Interrupted)));
    assert!(fs::read_dir(&fixture.rollback_root).unwrap().all(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with('.')
    }));
    let retry = export_chapter_rollback_bundle(
        &fixture.target,
        &fixture.legacy_backup,
        &fixture.rollback_root,
    )
    .unwrap();
    assert!(retry.bundle_path.is_dir());
}

fn imported_fixture() -> ChapterImportFixture {
    let fixture = ChapterImportFixture::new_v1();
    fixture.insert_episode(
        EPISODE_ID,
        PODCAST_ID,
        &episode_with_chapters(EPISODE_ID, true, false),
    );
    fixture.stage(1_800_000_200_000);
    fixture.verify(1_800_000_200_001);
    fixture.import(1_800_000_200_002);
    fixture
}
