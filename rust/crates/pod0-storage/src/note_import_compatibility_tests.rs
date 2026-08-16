use super::*;

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
