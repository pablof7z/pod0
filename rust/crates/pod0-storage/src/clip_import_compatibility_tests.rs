use super::*;

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
