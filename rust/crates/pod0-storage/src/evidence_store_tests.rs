use rusqlite::Connection;

use crate::evidence_store_test_support::*;
use crate::{EvidenceGenerationState, EvidenceStore, StorageError};

#[test]
fn complete_generation_reopens_identically_and_commands_are_idempotent() {
    let fixture = EvidenceFixture::new();
    let artifact = artifact("transcript-v3");

    let first_stage = fixture
        .store
        .stage_artifact(command(10), &artifact, 1_800_000_000_010)
        .unwrap();
    assert!(!first_stage.already_present);
    assert_eq!(
        fixture
            .store
            .stage_artifact(command(10), &artifact, 1_800_000_000_099)
            .unwrap(),
        first_stage
    );
    assert!(
        fixture
            .store
            .stage_artifact(command(11), &artifact, 1_800_000_000_011)
            .unwrap()
            .already_present
    );

    let first_verify = fixture
        .store
        .verify_generation(command(12), artifact.generation_id, 1_800_000_000_012)
        .unwrap();
    assert!(!first_verify.already_verified);
    assert_eq!(
        fixture
            .store
            .verify_generation(command(12), artifact.generation_id, 1_800_000_000_099)
            .unwrap(),
        first_verify
    );
    assert!(
        fixture
            .store
            .verify_generation(command(13), artifact.generation_id, 1_800_000_000_013)
            .unwrap()
            .already_verified
    );

    let first_select = fixture
        .store
        .select_generation(
            command(14),
            artifact.version.episode_id,
            artifact.generation_id,
            1_800_000_000_014,
        )
        .unwrap();
    assert!(!first_select.already_selected);
    assert_eq!(first_select.previous_generation_id, None);
    assert_eq!(
        fixture
            .store
            .select_generation(
                command(14),
                artifact.version.episode_id,
                artifact.generation_id,
                1_800_000_000_099,
            )
            .unwrap(),
        first_select
    );

    let reopened = EvidenceStore::open(&fixture.import.target).unwrap();
    assert_eq!(
        reopened
            .selected_artifact(artifact.version.episode_id)
            .unwrap(),
        Some(artifact.clone())
    );
    let summary = reopened
        .selected_generation(artifact.version.episode_id)
        .unwrap()
        .unwrap();
    assert_eq!(summary.state, EvidenceGenerationState::Verified);
    assert_eq!(summary.segment_count, 2);
    assert_eq!(summary.span_count, 1);
    let stored = reopened
        .generation(artifact.generation_id)
        .unwrap()
        .unwrap();
    assert_eq!(stored.spans[0].start_milliseconds, 47_125);
    assert_eq!(stored.spans[0].end_milliseconds, 60_000);
    assert_eq!(stored.spans[0].provenance, artifact.version.provenance);

    let connection = Connection::open(&fixture.import.target).unwrap();
    assert_eq!(count(&connection, "pod0_evidence_generations"), 1);
    assert_eq!(count(&connection, "pod0_evidence_spans"), 1);
    assert_eq!(count(&connection, "pod0_transcript_segments"), 2);
}

#[test]
fn selection_rollback_only_moves_the_pointer_and_prune_protects_selected_data() {
    let fixture = EvidenceFixture::new();
    let first = artifact("transcript-v3");
    let second = artifact("transcript-v4");
    stage_verify(&fixture.store, &first, 20);
    fixture
        .store
        .select_generation(
            command(22),
            first.version.episode_id,
            first.generation_id,
            1_800_000_000_022,
        )
        .unwrap();
    stage_verify(&fixture.store, &second, 30);
    let forward = fixture
        .store
        .select_generation(
            command(32),
            second.version.episode_id,
            second.generation_id,
            1_800_000_000_032,
        )
        .unwrap();
    assert_eq!(forward.previous_generation_id, Some(first.generation_id));
    assert!(
        fixture
            .store
            .generation(first.generation_id)
            .unwrap()
            .is_some()
    );

    let rollback = fixture
        .store
        .select_generation(
            command(33),
            first.version.episode_id,
            first.generation_id,
            1_800_000_000_033,
        )
        .unwrap();
    assert_eq!(rollback.previous_generation_id, Some(second.generation_id));
    assert_eq!(
        fixture
            .store
            .selected_artifact(first.version.episode_id)
            .unwrap(),
        Some(first.clone())
    );
    assert_eq!(
        fixture.store.prune_unselected_generation(
            command(34),
            first.generation_id,
            1_800_000_000_034
        ),
        Err(StorageError::EvidenceGenerationSelected)
    );
    assert!(
        fixture
            .store
            .prune_unselected_generation(command(35), second.generation_id, 1_800_000_000_035)
            .unwrap()
            .pruned
    );
    assert!(
        fixture
            .store
            .generation(second.generation_id)
            .unwrap()
            .is_none()
    );
    assert_eq!(
        fixture.store.generation(first.generation_id).unwrap(),
        Some(first)
    );
}

#[test]
fn stage_rejects_unsealed_or_unrepresentable_artifacts_without_writing_rows() {
    let fixture = EvidenceFixture::new();
    let mut unsealed = artifact("unsealed");
    unsealed.spans[0].text.push_str(" changed");
    assert_eq!(
        fixture
            .store
            .stage_artifact(command(40), &unsealed, 1_800_000_000_040),
        Err(StorageError::InvalidEvidenceArtifact)
    );

    let too_late = artifact_at(
        "timestamp-overflow",
        pod0_domain::PodcastId::from_bytes([0x11; 16]),
        i64::MAX as u64 + 1,
        i64::MAX as u64 + 101,
    );
    assert_eq!(
        fixture
            .store
            .stage_artifact(command(41), &too_late, 1_800_000_000_041),
        Err(StorageError::InvalidEvidenceArtifact)
    );

    let connection = Connection::open(&fixture.import.target).unwrap();
    assert_eq!(count(&connection, "pod0_transcript_documents"), 0);
    assert_eq!(count(&connection, "pod0_evidence_generations"), 0);
    assert_eq!(count(&connection, "pod0_evidence_commands"), 0);
}

fn stage_verify(
    store: &EvidenceStore,
    artifact: &pod0_domain::TranscriptEvidenceArtifact,
    id: u64,
) {
    store
        .stage_artifact(command(id), artifact, 1_800_000_000_000 + id as i64)
        .unwrap();
    store
        .verify_generation(
            command(id + 1),
            artifact.generation_id,
            1_800_000_000_001 + id as i64,
        )
        .unwrap();
}

fn count(connection: &Connection, table: &str) -> i64 {
    connection
        .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
            row.get(0)
        })
        .unwrap()
}
