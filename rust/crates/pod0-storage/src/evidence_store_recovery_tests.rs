use pod0_domain::PodcastId;
use rusqlite::{Connection, params};

use crate::evidence_store_test_support::*;
use crate::{EvidenceStore, StorageError};

#[test]
fn interruption_rolls_back_every_artifact_row_and_restart_is_clean() {
    let fixture = EvidenceFixture::new();
    let artifact = artifact("transcript-v3");
    assert_eq!(
        fixture.store.stage_artifact_with_observer(
            command(10),
            &artifact,
            1_800_000_000_010,
            || Err(StorageError::Interrupted),
        ),
        Err(StorageError::Interrupted)
    );

    let connection = Connection::open(&fixture.import.target).unwrap();
    assert_eq!(count(&connection, "pod0_transcript_documents"), 0);
    assert_eq!(count(&connection, "pod0_transcript_segments"), 0);
    assert_eq!(count(&connection, "pod0_evidence_generations"), 0);
    assert_eq!(count(&connection, "pod0_evidence_spans"), 0);
    assert_eq!(count(&connection, "pod0_evidence_commands"), 0);
    drop(connection);

    let reopened = EvidenceStore::open(&fixture.import.target).unwrap();
    assert!(
        reopened
            .generation(artifact.generation_id)
            .unwrap()
            .is_none()
    );
    assert!(
        !reopened
            .stage_artifact(command(10), &artifact, 1_800_000_000_011)
            .unwrap()
            .already_present
    );
}

#[test]
fn unverified_or_foreign_generation_cannot_replace_selection() {
    let fixture = EvidenceFixture::new();
    let first = artifact("transcript-v3");
    fixture
        .store
        .stage_artifact(command(20), &first, 1_800_000_000_020)
        .unwrap();
    assert_eq!(
        fixture.store.select_generation(
            command(21),
            first.version.episode_id,
            first.generation_id,
            1_800_000_000_021,
        ),
        Err(StorageError::EvidenceNotVerified)
    );
    fixture
        .store
        .verify_generation(command(22), first.generation_id, 1_800_000_000_022)
        .unwrap();
    fixture
        .store
        .select_generation(
            command(23),
            first.version.episode_id,
            first.generation_id,
            1_800_000_000_023,
        )
        .unwrap();

    let second = artifact("transcript-v4");
    fixture
        .store
        .stage_artifact(command(24), &second, 1_800_000_000_024)
        .unwrap();
    assert_eq!(
        fixture.store.select_generation(
            command(25),
            second.version.episode_id,
            second.generation_id,
            1_800_000_000_025,
        ),
        Err(StorageError::EvidenceNotVerified)
    );
    assert_eq!(
        fixture
            .store
            .selected_generation(first.version.episode_id)
            .unwrap()
            .unwrap()
            .generation_id,
        first.generation_id
    );

    let foreign = artifact_for_podcast("foreign", PodcastId::from_parts(9, 9));
    assert_eq!(
        fixture
            .store
            .stage_artifact(command(26), &foreign, 1_800_000_000_026),
        Err(StorageError::EvidenceEpisodeMismatch)
    );
    assert_eq!(
        fixture
            .store
            .stage_artifact(command(20), &second, 1_800_000_000_027),
        Err(StorageError::EvidenceCommandConflict)
    );
}

#[test]
fn digest_reference_newer_schema_and_incomplete_corruption_fail_closed() {
    let digest_fixture = EvidenceFixture::new();
    let digest_artifact = artifact("digest");
    digest_fixture
        .store
        .stage_artifact(command(30), &digest_artifact, 1_800_000_000_030)
        .unwrap();
    Connection::open(&digest_fixture.import.target)
        .unwrap()
        .execute(
            "UPDATE pod0_evidence_spans SET text='changed but structurally valid' \
             WHERE generation_id=?1",
            [digest_artifact.generation_id.into_bytes().as_slice()],
        )
        .unwrap();
    assert_eq!(
        digest_fixture
            .store
            .generation(digest_artifact.generation_id),
        Err(StorageError::InvalidEvidenceArtifact)
    );

    let reference_fixture = EvidenceFixture::new();
    let reference_artifact = artifact("reference");
    reference_fixture
        .store
        .stage_artifact(command(31), &reference_artifact, 1_800_000_000_031)
        .unwrap();
    let reference_connection = Connection::open(&reference_fixture.import.target).unwrap();
    reference_connection
        .pragma_update(None, "foreign_keys", "OFF")
        .unwrap();
    reference_connection
        .execute(
            "UPDATE pod0_evidence_spans SET first_segment_id=?1 WHERE generation_id=?2",
            params![
                [0x99_u8; 16].as_slice(),
                reference_artifact.generation_id.into_bytes().as_slice()
            ],
        )
        .unwrap();
    drop(reference_connection);
    assert_eq!(
        reference_fixture
            .store
            .generation(reference_artifact.generation_id),
        Err(StorageError::InvalidEvidenceArtifact)
    );
    assert!(matches!(
        EvidenceStore::open(&reference_fixture.import.target),
        Err(StorageError::InvalidEvidenceArtifact)
    ));

    let newer_fixture = EvidenceFixture::new();
    let newer_artifact = artifact("newer");
    newer_fixture
        .store
        .stage_artifact(command(32), &newer_artifact, 1_800_000_000_032)
        .unwrap();
    Connection::open(&newer_fixture.import.target)
        .unwrap()
        .execute(
            "UPDATE pod0_evidence_generations SET artifact_schema_version=2 \
             WHERE generation_id=?1",
            [newer_artifact.generation_id.into_bytes().as_slice()],
        )
        .unwrap();
    assert_eq!(
        newer_fixture.store.generation(newer_artifact.generation_id),
        Err(StorageError::NewerEvidenceSchema {
            stored: 2,
            supported: 1,
        })
    );

    let incomplete_fixture = EvidenceFixture::new();
    let incomplete_artifact = artifact("incomplete");
    incomplete_fixture
        .store
        .stage_artifact(command(33), &incomplete_artifact, 1_800_000_000_033)
        .unwrap();
    Connection::open(&incomplete_fixture.import.target)
        .unwrap()
        .execute(
            "DELETE FROM pod0_evidence_spans WHERE generation_id=?1",
            [incomplete_artifact.generation_id.into_bytes().as_slice()],
        )
        .unwrap();
    assert_eq!(
        incomplete_fixture.store.verify_generation(
            command(34),
            incomplete_artifact.generation_id,
            1_800_000_000_034
        ),
        Err(StorageError::InvalidEvidenceArtifact)
    );
}

fn count(connection: &Connection, table: &str) -> i64 {
    connection
        .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
            row.get(0)
        })
        .unwrap()
}
