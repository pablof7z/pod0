use std::fs;

use pod0_domain::{ClipId, ClipRevision, ClipSource, EpisodeId, PodcastId};

use crate::listening_import_test_support::*;
use crate::{
    ClipImportClock, ClipImporter, LibraryStore, NoteImportClock, NoteImporter, StorageError,
    commit_clip_cutover, commit_listening_cutover, commit_note_cutover, inspect_legacy_clip_source,
    inspect_legacy_note_source, read_clip_import,
};

struct OrphanClock;

const UPDATE_FINGERPRINT: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const CREATE_FINGERPRINT: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

impl ClipImportClock for OrphanClock {
    fn now_milliseconds(&self) -> i64 {
        1_721_323_000_100
    }
}

impl NoteImportClock for OrphanClock {
    fn now_milliseconds(&self) -> i64 {
        1_721_323_000_100
    }
}

#[test]
fn legacy_orphan_clip_references_survive_cutover_exactly() {
    let fixture = ImportFixture::new();
    let mut metadata = current_metadata(8);
    metadata["episodes"] = serde_json::json!([]);
    metadata["notes"] = serde_json::json!([]);
    metadata["clips"] = serde_json::json!([{
        "id": "12121212-1212-1212-1212-121212121212",
        "episodeID": "34343434-3434-3434-3434-343434343434",
        "subscriptionID": "56565656-5656-5656-5656-565656565656",
        "startMs": 10,
        "endMs": 20,
        "speakerID": "Speaker One",
        "transcriptText": "Preserved after its podcast was removed"
    }]);
    fs::write(&fixture.source, serde_json::to_vec(&metadata).unwrap()).unwrap();
    let listening = fixture.plan();
    fixture.stage(&listening).unwrap();
    assert!(!commit_listening_cutover(&fixture.target, 1_721_323_000_001).unwrap());
    let notes = inspect_legacy_note_source(&fixture.source).unwrap();
    NoteImporter::new(OrphanClock)
        .stage(
            &fixture.source,
            &fixture._directory.path().join("notes.backup.json"),
            &fixture.target,
            &fixture.target_backup,
            &notes,
            id(3),
            id(4),
        )
        .unwrap();
    assert!(!commit_note_cutover(&fixture.target, 1_721_323_000_002).unwrap());
    let plan = inspect_legacy_clip_source(&fixture.source).unwrap();
    ClipImporter::new(OrphanClock)
        .stage(
            &fixture.source,
            &fixture._directory.path().join("orphan-clips.backup.json"),
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
    assert_eq!(clip.episode_id.into_bytes(), [0x34; 16]);
    assert_eq!(clip.podcast_id.into_bytes(), [0x56; 16]);
    assert_eq!(clip.speaker_label.as_deref(), Some("Speaker One"));
    assert!(clip.speaker_id.is_none());
    assert_eq!(
        clip.frozen_transcript_text,
        "Preserved after its podcast was removed"
    );

    assert!(!commit_clip_cutover(&fixture.source, &fixture.target, 1_721_323_000_003).unwrap());
    let store = LibraryStore::open_authoritative(&fixture.target).unwrap();
    store
        .update_clip(
            id(7),
            UPDATE_FINGERPRINT,
            clip.clip_id,
            ClipRevision::INITIAL,
            11,
            21,
            None,
            None,
            "Updated without rewriting its speaker label",
            1_721_323_000_004,
        )
        .unwrap();
    let updated = store.clip_snapshot().unwrap().clips.remove(0);
    assert_eq!(updated.speaker_label.as_deref(), Some("Speaker One"));
    assert_eq!(
        store
            .create_clip(
                id(6),
                CREATE_FINGERPRINT,
                ClipId::from_parts(7, 8),
                EpisodeId::from_parts(9, 10),
                PodcastId::from_parts(11, 12),
                10,
                20,
                None,
                None,
                "New clips still require a live target",
                ClipSource::Touch,
                1_721_323_000_005,
            )
            .unwrap_err(),
        StorageError::InvalidClip
    );
}
