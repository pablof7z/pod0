use std::fs;

use rusqlite::Connection;

use crate::listening_import_test_support::*;
use crate::{
    LibraryStore, NoteImportClock, NoteImporter, StorageError, commit_listening_cutover,
    commit_note_cutover, inspect_legacy_note_source, read_note_import,
};

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

    assert!(!commit_note_cutover(&fixture.target, 1_721_322_000_101).unwrap());
    assert!(commit_note_cutover(&fixture.target, 1_721_322_000_102).unwrap());
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

#[test]
fn older_json_notes_receive_legacy_defaults_without_identity_or_time_drift() {
    let fixture = ImportFixture::new();
    let metadata = serde_json::json!({
        "persistenceGeneration": 5,
        "podcasts": [],
        "subscriptions": [],
        "episodes": [],
        "settings": {},
        "notes": [{
            "id": "55555555-5555-5555-5555-555555555555",
            "text": "Old note",
            "createdAt": 725846400.25
        }]
    });
    prepare_json_listening(&fixture, &metadata);
    let plan = inspect_legacy_note_source(&fixture.source).unwrap();
    let backup = fixture._directory.path().join("old-notes.backup.json");
    NoteImporter::new(FixedClock)
        .stage(
            &fixture.source,
            &backup,
            &fixture.target,
            &fixture.target_backup,
            &plan,
            id(3),
            id(4),
        )
        .unwrap();
    let note = read_note_import(&fixture.target, id(3))
        .unwrap()
        .snapshot
        .notes
        .into_iter()
        .next()
        .unwrap();
    assert_eq!(note.note_id.into_bytes(), [0x55; 16]);
    assert_eq!(note.kind, pod0_domain::NoteKind::Free);
    assert_eq!(note.author, pod0_domain::NoteAuthor::User);
    assert_eq!(note.created_at.value, 1_704_153_600_250);
    assert!(!note.deleted && note.target.is_none());
}

#[test]
fn changed_ambiguous_and_future_note_sources_fail_closed() {
    let changed = ImportFixture::new();
    let original = serde_json::json!({
        "persistenceGeneration": 6,
        "podcasts": [], "subscriptions": [], "episodes": [], "settings": {},
        "notes": []
    });
    prepare_json_listening(&changed, &original);
    let plan = inspect_legacy_note_source(&changed.source).unwrap();
    let mut edited = original.clone();
    edited["notes"] = serde_json::json!([{
        "id": "66666666-6666-6666-6666-666666666666",
        "text": "Arrived after inspection",
        "createdAt": "2024-01-01T00:00:00Z"
    }]);
    fs::write(&changed.source, serde_json::to_vec(&edited).unwrap()).unwrap();
    assert_eq!(
        NoteImporter::new(FixedClock)
            .stage(
                &changed.source,
                &changed._directory.path().join("changed.backup.json"),
                &changed.target,
                &changed.target_backup,
                &plan,
                id(3),
                id(4),
            )
            .unwrap_err(),
        StorageError::SourceChanged
    );
    assert_eq!(
        Connection::open(&changed.target)
            .unwrap()
            .query_row("SELECT COUNT(*) FROM pod0_notes", [], |row| row
                .get::<_, u32>(0))
            .unwrap(),
        0
    );

    for notes in [
        serde_json::json!([
            {"id":"77777777-7777-7777-7777-777777777777","text":"one","createdAt":"2024-01-01T00:00:00Z"},
            {"id":"77777777-7777-7777-7777-777777777777","text":"two","createdAt":"2024-01-02T00:00:00Z"}
        ]),
        serde_json::json!([
            {"id":"88888888-8888-8888-8888-888888888888","text":"future","kind":"futureKind","createdAt":"2024-01-01T00:00:00Z"}
        ]),
    ] {
        let invalid = ImportFixture::new();
        let mut metadata = original.clone();
        metadata["notes"] = notes;
        fs::write(&invalid.source, serde_json::to_vec(&metadata).unwrap()).unwrap();
        assert!(matches!(
            inspect_legacy_note_source(&invalid.source),
            Err(StorageError::InvalidLegacyRecord { .. })
        ));
    }
}
