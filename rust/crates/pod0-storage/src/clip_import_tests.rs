use std::fs;

use rusqlite::Connection;

use crate::listening_import_test_support::*;
use crate::{
    ClipImportClock, ClipImporter, LibraryStore, NoteImporter, StorageError, commit_clip_cutover,
    commit_listening_cutover, commit_note_cutover, inspect_legacy_clip_source,
    inspect_legacy_note_source, read_clip_import,
};

impl ClipImportClock for FixedClock {
    fn now_milliseconds(&self) -> i64 {
        1_721_323_000_100
    }
}

fn prepare_prerequisites(fixture: &ImportFixture, metadata: &serde_json::Value) {
    create_sqlite_source(&fixture.source, metadata, &[episode(EPISODE_ID, "guid-1")]);
    let plan = fixture.plan();
    fixture.stage(&plan).unwrap();
    assert!(!commit_listening_cutover(&fixture.target, 1_721_323_000_001).unwrap());
    let note_plan = inspect_legacy_note_source(&fixture.source).unwrap();
    NoteImporter::new(FixedClock)
        .stage(
            &fixture.source,
            &fixture._directory.path().join("notes.backup.sqlite"),
            &fixture.target,
            &fixture.target_backup,
            &note_plan,
            id(3),
            id(4),
        )
        .unwrap();
    assert!(!commit_note_cutover(&fixture.target, 1_721_323_000_002).unwrap());
}

pub(crate) fn prepare_json_prerequisites(fixture: &ImportFixture, metadata: &serde_json::Value) {
    fs::write(&fixture.source, serde_json::to_vec(metadata).unwrap()).unwrap();
    let plan = fixture.plan();
    fixture.stage(&plan).unwrap();
    assert!(!commit_listening_cutover(&fixture.target, 1_721_323_000_001).unwrap());
    let note_plan = inspect_legacy_note_source(&fixture.source).unwrap();
    NoteImporter::new(FixedClock)
        .stage(
            &fixture.source,
            &fixture._directory.path().join("notes.backup.json"),
            &fixture.target,
            &fixture.target_backup,
            &note_plan,
            id(3),
            id(4),
        )
        .unwrap();
    assert!(!commit_note_cutover(&fixture.target, 1_721_323_000_002).unwrap());
}

pub(crate) fn metadata_with_clips() -> serde_json::Value {
    let mut metadata = current_metadata(51);
    metadata["clips"] = serde_json::json!([
        {
            "id": "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa",
            "episodeID": EPISODE_ID,
            "subscriptionID": PODCAST_ID,
            "startMs": 12345,
            "endMs": 15345,
            "createdAt": 725846400.125,
            "caption": "First moment",
            "speakerID": "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb",
            "transcriptText": "Frozen first words",
            "source": "touch"
        },
        {
            "id": "cccccccc-cccc-cccc-cccc-cccccccccccc",
            "episodeID": EPISODE_ID,
            "subscriptionID": PODCAST_ID,
            "startMs": 20000,
            "endMs": 23000,
            "createdAt": "2024-01-03T00:00:00Z",
            "transcriptText": "Frozen agent words",
            "source": "agent",
            "deleted": true
        }
    ]);
    metadata
}

#[test]
fn swift_clips_are_backed_up_staged_verified_and_reopened_losslessly() {
    let fixture = ImportFixture::new();
    prepare_prerequisites(&fixture, &metadata_with_clips());
    let backup = fixture._directory.path().join("clips.backup.sqlite");
    let plan = inspect_legacy_clip_source(&fixture.source).unwrap();
    assert_eq!(plan.clip_count, 2);
    let source_bytes = fs::read(&fixture.source).unwrap();
    let importer = ClipImporter::new(FixedClock);

    let first = importer
        .stage(
            &fixture.source,
            &backup,
            &fixture.target,
            &fixture.target_backup,
            &plan,
            id(5),
            id(4),
        )
        .unwrap();
    assert!(first.staged && !first.reused_existing);
    assert_eq!(fs::read(&fixture.source).unwrap(), source_bytes);
    assert_eq!(inspect_legacy_clip_source(&backup).unwrap(), plan);
    assert!(
        importer
            .stage(
                &fixture.source,
                &backup,
                &fixture.target,
                &fixture.target_backup,
                &plan,
                id(5),
                id(4),
            )
            .unwrap()
            .reused_existing
    );

    let verification = read_clip_import(&fixture.target, id(5)).unwrap();
    assert_eq!(verification.snapshot.clips.len(), 2);
    assert_eq!(
        verification
            .snapshot
            .clips
            .iter()
            .map(|clip| clip.frozen_transcript_text.as_str())
            .collect::<Vec<_>>(),
        ["Frozen agent words", "Frozen first words"]
    );
    let first = verification
        .snapshot
        .clips
        .iter()
        .find(|clip| clip.caption.as_deref() == Some("First moment"))
        .unwrap();
    assert_eq!(first.start_milliseconds, 12_345);
    assert_eq!(first.created_at.value, 1_704_153_600_125);
    assert_eq!(first.speaker_id.unwrap().into_bytes(), [0xbb; 16]);
    assert!(verification.snapshot.clips[0].deleted);

    assert!(!commit_clip_cutover(&fixture.source, &fixture.target, 1_721_323_000_101).unwrap());
    assert!(commit_clip_cutover(&fixture.source, &fixture.target, 1_721_323_000_102).unwrap());
    let reopened = LibraryStore::open_authoritative(&fixture.target).unwrap();
    assert_eq!(reopened.clip_snapshot().unwrap(), verification.snapshot);
}

#[test]
fn interrupted_clip_import_rolls_back_and_retry_recovers() {
    let fixture = ImportFixture::new();
    prepare_prerequisites(&fixture, &metadata_with_clips());
    let backup = fixture._directory.path().join("clips.backup.sqlite");
    let plan = inspect_legacy_clip_source(&fixture.source).unwrap();
    let importer = ClipImporter::new(FixedClock);
    assert_eq!(
        importer
            .stage_with_observer(
                &fixture.source,
                &backup,
                &fixture.target,
                &fixture.target_backup,
                &plan,
                id(5),
                id(4),
                || Err(StorageError::Interrupted),
            )
            .unwrap_err(),
        StorageError::Interrupted
    );
    assert_eq!(
        Connection::open(&fixture.target)
            .unwrap()
            .query_row("SELECT COUNT(*) FROM pod0_clip_imports", [], |row| row
                .get::<_, u32>(0))
            .unwrap(),
        0
    );
    assert!(
        importer
            .stage(
                &fixture.source,
                &backup,
                &fixture.target,
                &fixture.target_backup,
                &plan,
                id(5),
                id(4),
            )
            .unwrap()
            .staged
    );
}

#[test]
fn older_defaults_and_changed_or_ambiguous_clip_sources_are_deterministic() {
    let fixture = ImportFixture::new();
    let mut metadata = current_metadata(5);
    metadata["episodes"] = serde_json::json!([episode(EPISODE_ID, "guid-1")]);
    metadata["notes"] = serde_json::json!([]);
    metadata["clips"] = serde_json::json!([{
        "id": "dddddddd-dddd-dddd-dddd-dddddddddddd",
        "episodeID": EPISODE_ID,
        "subscriptionID": PODCAST_ID,
        "startMs": 10, "endMs": 20
    }]);
    prepare_json_prerequisites(&fixture, &metadata);
    let plan = inspect_legacy_clip_source(&fixture.source).unwrap();
    ClipImporter::new(FixedClock)
        .stage(
            &fixture.source,
            &fixture._directory.path().join("old-clips.backup.json"),
            &fixture.target,
            &fixture.target_backup,
            &plan,
            id(5),
            id(4),
        )
        .unwrap();
    let clip = read_clip_import(&fixture.target, id(5))
        .unwrap()
        .snapshot
        .clips
        .remove(0);
    assert_eq!(clip.clip_id.into_bytes(), [0xdd; 16]);
    assert_eq!(clip.created_at.value, 0);
    assert_eq!(clip.source, pod0_domain::ClipSource::Touch);
    assert!(clip.frozen_transcript_text.is_empty());

    let changed = ImportFixture::new();
    prepare_json_prerequisites(&changed, &metadata);
    let inspected = inspect_legacy_clip_source(&changed.source).unwrap();
    let mut edited = metadata.clone();
    edited["clips"][0]["endMs"] = serde_json::json!(21);
    fs::write(&changed.source, serde_json::to_vec(&edited).unwrap()).unwrap();
    assert_eq!(
        ClipImporter::new(FixedClock)
            .stage(
                &changed.source,
                &changed._directory.path().join("changed.backup.json"),
                &changed.target,
                &changed.target_backup,
                &inspected,
                id(5),
                id(4),
            )
            .unwrap_err(),
        StorageError::SourceChanged
    );

    for clips in [
        serde_json::json!([
            {"id":"eeeeeeee-eeee-eeee-eeee-eeeeeeeeeeee","episodeID":EPISODE_ID,"subscriptionID":PODCAST_ID,"startMs":1,"endMs":2},
            {"id":"eeeeeeee-eeee-eeee-eeee-eeeeeeeeeeee","episodeID":EPISODE_ID,"subscriptionID":PODCAST_ID,"startMs":2,"endMs":3}
        ]),
        serde_json::json!([
            {"id":"ffffffff-ffff-ffff-ffff-ffffffffffff","episodeID":EPISODE_ID,"subscriptionID":PODCAST_ID,"startMs":5,"endMs":5}
        ]),
        serde_json::json!([
            {"id":"11111111-aaaa-aaaa-aaaa-aaaaaaaaaaaa","episodeID":EPISODE_ID,"subscriptionID":PODCAST_ID,"startMs":1,"endMs":2,"source":"future"}
        ]),
    ] {
        let invalid = ImportFixture::new();
        let mut value = metadata.clone();
        value["clips"] = clips;
        fs::write(&invalid.source, serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(matches!(
            inspect_legacy_clip_source(&invalid.source),
            Err(StorageError::InvalidLegacyRecord { .. })
        ));
    }
}
