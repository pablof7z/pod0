use crate::transcript_import_test_support::{FixedTranscriptClock, command};
use crate::{
    LegacyTranscriptSourceKind, TranscriptImporter, TranscriptStore, commit_listening_cutover,
    inspect_legacy_transcript_source,
};

#[test]
fn fresh_install_without_legacy_artifact_table_cuts_over_as_empty() {
    let import = crate::listening_import_test_support::ImportFixture::new();
    crate::listening_import_test_support::create_sqlite_source(
        &import.source,
        &crate::listening_import_test_support::current_metadata(1),
        &[crate::listening_import_test_support::episode(
            crate::listening_import_test_support::EPISODE_ID,
            "empty-transcript-source",
        )],
    );
    import.stage(&import.plan()).unwrap();
    commit_listening_cutover(&import.target, 1_800_000_000_000).unwrap();
    let root = import._directory.path().join("missing-transcript-root");
    let backups = import._directory.path().join("transcript-backups");
    let plan = inspect_legacy_transcript_source(&import.source, &root).unwrap();
    assert_eq!(
        plan.source_kind,
        LegacyTranscriptSourceKind::ArtifactSqliteV1
    );
    assert_eq!(plan.selected_count, 0);
    let importer = TranscriptImporter::new(FixedTranscriptClock);
    importer
        .stage(
            &import.source,
            &root,
            &backups,
            &import.target,
            &import.target_backup,
            &plan,
            command(1_000),
            command(1_001),
        )
        .unwrap();
    let verified = importer
        .verify(&import.target, &backups, command(1_000))
        .unwrap();
    assert_eq!(verified.verified_artifact_count, 0);
    importer
        .commit(&import.source, &root, &import.target, command(1_000))
        .unwrap();
    assert!(TranscriptStore::open_authoritative(&import.target).is_ok());
}
