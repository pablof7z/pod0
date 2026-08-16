use std::fs;

use pod0_application::{
    ActivityActor, ActivityFact, ActivityOrigin, CommandActivityIdentity,
    user_artifact_migration_command_id,
};
use rusqlite::Connection;

use crate::listening_import_test_support::*;
use crate::{
    LibraryStore, NoteImportClock, NoteImporter, StorageError, commit_listening_cutover,
    commit_note_cutover, inspect_legacy_note_source, read_note_import,
};

#[path = "note_import_compatibility_tests.rs"]
mod compatibility;

impl NoteImportClock for FixedClock {
    fn now_milliseconds(&self) -> i64 {
        1_721_322_000_100
    }
}

fn prepare_listening(fixture: &ImportFixture, metadata: &serde_json::Value) {
    create_sqlite_source(&fixture.source, metadata, &[episode(EPISODE_ID, "guid-1")]);
    let plan = fixture.plan();
    fixture.stage(&plan).unwrap();
    assert!(!commit_listening_cutover(&fixture.target, 1_721_322_000_001).unwrap());
}

fn prepare_json_listening(fixture: &ImportFixture, metadata: &serde_json::Value) {
    fs::write(&fixture.source, serde_json::to_vec(metadata).unwrap()).unwrap();
    let plan = fixture.plan();
    fixture.stage(&plan).unwrap();
    assert!(!commit_listening_cutover(&fixture.target, 1_721_322_000_001).unwrap());
}

fn metadata_with_notes() -> serde_json::Value {
    let mut metadata = current_metadata(41);
    metadata["notes"] = serde_json::json!([
        {
            "id": "33333333-3333-3333-3333-333333333333",
            "text": "A durable thought",
            "kind": "reflection",
            "target": {
                "kind": "episode",
                "id": EPISODE_ID,
                "positionSeconds": 12.345
            },
            "createdAt": 725846400.0,
            "deleted": false,
            "author": "user"
        },
        {
            "id": "44444444-4444-4444-4444-444444444444",
            "text": "Agent follow-up",
            "kind": "free",
            "createdAt": "2024-01-03T00:00:00Z",
            "deleted": true,
            "author": "agent"
        }
    ]);
    metadata
}

fn note_cutover_activity(
    fixture: &ImportFixture,
) -> Vec<pod0_application::CommittedActivityFact> {
    let command_id = user_artifact_migration_command_id("notes", "commit", id(3));
    let correlation = CommandActivityIdentity::new(command_id).correlation_id();
    crate::ActivityStore::open(&fixture.target)
        .unwrap()
        .page_for_correlation(correlation, None, 20)
        .unwrap()
        .items
}

#[test]
fn swift_notes_are_backed_up_staged_verified_and_reopened_losslessly() {
    let fixture = ImportFixture::new();
    prepare_listening(&fixture, &metadata_with_notes());
    let note_backup = fixture._directory.path().join("notes.backup.sqlite");
    let plan = inspect_legacy_note_source(&fixture.source).unwrap();
    assert_eq!(plan.note_count, 2);
    let source_bytes = fs::read(&fixture.source).unwrap();
    let importer = NoteImporter::new(FixedClock);

    let first = importer
        .stage(
            &fixture.source,
            &note_backup,
            &fixture.target,
            &fixture.target_backup,
            &plan,
            id(3),
            id(4),
        )
        .unwrap();
    assert!(first.staged && !first.reused_existing);
    assert_eq!(fs::read(&fixture.source).unwrap(), source_bytes);
    assert_eq!(inspect_legacy_note_source(&note_backup).unwrap(), plan);

    let retry = importer
        .stage(
            &fixture.source,
            &note_backup,
            &fixture.target,
            &fixture.target_backup,
            &plan,
            id(3),
            id(4),
        )
        .unwrap();
    assert!(retry.reused_existing);

    let verification = read_note_import(&fixture.target, id(3)).unwrap();
    assert_eq!(verification.snapshot.notes.len(), 2);
    assert_eq!(
        verification
            .snapshot
            .notes
            .iter()
            .map(|note| note.text.as_str())
            .collect::<Vec<_>>(),
        ["Agent follow-up", "A durable thought"]
    );
    let reflection = verification
        .snapshot
        .notes
        .iter()
        .find(|note| note.text == "A durable thought")
        .unwrap();
    assert_eq!(reflection.created_at.value, 1_704_153_600_000);
    assert!(matches!(
        reflection.target,
        Some(pod0_domain::NoteTarget::Episode {
            position_milliseconds: 12_345,
            ..
        })
    ));

    assert_eq!(
        LibraryStore::open_authoritative(&fixture.target)
            .unwrap()
            .note_snapshot()
            .unwrap_err(),
        StorageError::CutoverNotAuthoritative
    );
    assert!(note_cutover_activity(&fixture).is_empty());
    assert!(!commit_note_cutover(&fixture.target, 1_721_322_000_101).unwrap());
    let activity = note_cutover_activity(&fixture);
    assert_eq!(activity.len(), 3);
    assert!(activity.iter().all(|fact| {
        fact.draft.actor == ActivityActor::Migration
            && fact.draft.origin == ActivityOrigin::Migration
    }));
    assert!(matches!(
        activity[2].draft.fact,
        ActivityFact::AuthorityCutover { .. }
    ));
    assert!(commit_note_cutover(&fixture.target, 1_721_322_000_102).unwrap());
    assert_eq!(note_cutover_activity(&fixture), activity);
    let reopened = LibraryStore::open_authoritative(&fixture.target).unwrap();
    assert_eq!(reopened.note_snapshot().unwrap(), verification.snapshot);
}

#[test]
fn interrupted_note_import_rolls_back_and_retry_recovers() {
    let fixture = ImportFixture::new();
    prepare_listening(&fixture, &metadata_with_notes());
    let note_backup = fixture._directory.path().join("notes.backup.sqlite");
    let plan = inspect_legacy_note_source(&fixture.source).unwrap();
    let importer = NoteImporter::new(FixedClock);

    assert_eq!(
        importer
            .stage_with_observer(
                &fixture.source,
                &note_backup,
                &fixture.target,
                &fixture.target_backup,
                &plan,
                id(3),
                id(4),
                || Err(StorageError::Interrupted),
            )
            .unwrap_err(),
        StorageError::Interrupted
    );
    let connection = Connection::open(&fixture.target).unwrap();
    assert_eq!(
        connection
            .query_row("SELECT COUNT(*) FROM pod0_note_imports", [], |row| row
                .get::<_, u32>(0))
            .unwrap(),
        0
    );
    drop(connection);

    assert!(
        importer
            .stage(
                &fixture.source,
                &note_backup,
                &fixture.target,
                &fixture.target_backup,
                &plan,
                id(3),
                id(4),
            )
            .unwrap()
            .staged
    );
}
