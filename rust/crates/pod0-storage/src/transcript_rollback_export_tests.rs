use pod0_domain::{StateRevision, TranscriptArtifact};

use crate::transcript_store_test_support::{TranscriptFixture, command, input};
use crate::{StorageError, export_transcript_rollback_bundle, inspect_legacy_transcript_source};

#[test]
fn authoritative_selection_exports_as_versioned_verified_legacy_bundle() {
    let fixture = TranscriptFixture::new();
    let expected = TranscriptArtifact::seal(input("rollback-v1")).unwrap();
    fixture
        .store
        .commit_and_select(
            command(900),
            StateRevision::INITIAL,
            input("rollback-v1"),
            1_800_000_000_900,
        )
        .unwrap();
    let root = fixture.import._directory.path().join("rollback-exports");

    let first = export_transcript_rollback_bundle(&fixture.import.target, &root).unwrap();
    assert_eq!(first.core_schema_version, 12);
    assert_eq!(first.transcript_revision, 2);
    assert_eq!(first.artifact_count, 1);
    assert_eq!(first.selected_count, 1);
    assert!(!first.reused_existing);
    assert!(
        first
            .bundle_path
            .ends_with("transcripts-v1-core-v12-revision-2")
    );

    let plan = inspect_legacy_transcript_source(
        &first.bundle_path.join("transcript-selection.sqlite"),
        &first.bundle_path.join("transcripts"),
    )
    .unwrap();
    assert_eq!(plan.source_generation, 2);
    assert_eq!(plan.artifact_count, 1);
    assert_eq!(plan.selected_count, 1);
    let exported = std::fs::read_to_string(
        first
            .bundle_path
            .join("transcripts/artifacts")
            .join(uuid(expected.episode_id.into_bytes()))
            .join(format!("{}.json", hex(expected.artifact_id.into_bytes()))),
    )
    .unwrap();
    assert!(exported.contains("Small   habits become durable"));
    assert!(exported.contains("\"speakerID\""));

    let replay = export_transcript_rollback_bundle(&fixture.import.target, &root).unwrap();
    assert!(replay.reused_existing);
    assert_eq!(replay.bundle_path, first.bundle_path);
}

#[test]
fn rollback_bundle_preserves_every_artifact_and_the_exact_selection() {
    let fixture = TranscriptFixture::new();
    let first_artifact = TranscriptArtifact::seal(input("rollback-history-v1")).unwrap();
    let selected_artifact = TranscriptArtifact::seal(input("rollback-history-v2")).unwrap();
    fixture
        .store
        .commit_and_select(
            command(910),
            StateRevision::INITIAL,
            input("rollback-history-v1"),
            1_800_000_000_910,
        )
        .unwrap();
    fixture
        .store
        .commit_and_select(
            command(911),
            StateRevision::new(1),
            input("rollback-history-v2"),
            1_800_000_000_911,
        )
        .unwrap();
    let root = fixture.import._directory.path().join("rollback-history");

    let report = export_transcript_rollback_bundle(&fixture.import.target, &root).unwrap();
    assert_eq!(report.transcript_revision, 3);
    assert_eq!(report.artifact_count, 2);
    assert_eq!(report.selected_count, 1);

    let plan = inspect_legacy_transcript_source(
        &report.bundle_path.join("transcript-selection.sqlite"),
        &report.bundle_path.join("transcripts"),
    )
    .unwrap();
    assert_eq!(plan.artifact_count, 2);
    assert_eq!(plan.selected_count, 1);
    let connection =
        rusqlite::Connection::open(report.bundle_path.join("transcript-selection.sqlite")).unwrap();
    let selected_output: String = connection
        .query_row(
            "SELECT output_version FROM artifacts WHERE kind='transcript' AND selected=1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        selected_output,
        hex(selected_artifact.artifact_id.into_bytes())
    );
    let historical_output: String = connection
        .query_row(
            "SELECT output_version FROM artifacts WHERE kind='transcript' AND selected=0",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        historical_output,
        hex(first_artifact.artifact_id.into_bytes())
    );
}

#[test]
fn tampered_existing_rollback_bundle_fails_closed() {
    let fixture = TranscriptFixture::new();
    fixture
        .store
        .commit_and_select(
            command(901),
            StateRevision::INITIAL,
            input("rollback-v1"),
            1_800_000_000_901,
        )
        .unwrap();
    let root = fixture.import._directory.path().join("rollback-exports");
    let report = export_transcript_rollback_bundle(&fixture.import.target, &root).unwrap();
    std::fs::write(report.bundle_path.join("manifest.json"), b"{}").unwrap();

    assert_eq!(
        export_transcript_rollback_bundle(&fixture.import.target, &root),
        Err(StorageError::BackupConflict)
    );
}

fn uuid(bytes: [u8; 16]) -> String {
    let hex = hex(bytes);
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

fn hex(bytes: [u8; 16]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
