use pod0_domain::{CancellationId, CommandId, ContentDigest, StateRevision};

use super::*;
use crate::transcript_store_test_support::TranscriptFixture;
use crate::{LibraryStore, StorageError};

const NOW: i64 = 1_800_000_300_000;

#[test]
fn authority_commit_is_atomic_and_restart_safe() {
    let fixture = TranscriptFixture::new();
    let store = LibraryStore::open_authoritative(&fixture.import.target).unwrap();
    let rows = Vec::new();
    let fingerprint = transcript_workflow_source_fingerprint(&rows);
    store
        .stage_legacy_transcript_workflow_cutover(LegacyTranscriptWorkflowCutoverInput {
            source_generation: 10,
            source_fingerprint: fingerprint,
            backup_digest: ContentDigest::from_bytes([0xA1; 32]),
            backup_byte_count: 0,
            rows,
            candidates: Vec::new(),
            command_id: CommandId::from_parts(10, 1),
            cancellation_id: CancellationId::from_parts(10, 2),
            issued_revision: StateRevision::new(10),
            max_attempts: 8,
            now_ms: NOW,
        })
        .unwrap();
    store
        .verify_legacy_transcript_workflow_cutover(10, fingerprint, NOW + 1)
        .unwrap();

    assert_eq!(
        store.commit_legacy_transcript_workflow_cutover_with_observer(
            10,
            fingerprint,
            NOW + 2,
            || Err(StorageError::Interrupted),
        ),
        Err(StorageError::Interrupted)
    );
    let reopened = LibraryStore::open_authoritative(&fixture.import.target).unwrap();
    assert_eq!(
        reopened.transcript_workflow_authority().unwrap(),
        TranscriptWorkflowAuthorityState::Verified {
            source_generation: 10
        }
    );
    assert_eq!(
        reopened
            .export_legacy_transcript_workflow_rollback()
            .unwrap()
            .rows,
        []
    );

    reopened
        .commit_legacy_transcript_workflow_cutover(10, fingerprint, NOW + 2)
        .unwrap();
    let committed = LibraryStore::open_authoritative(&fixture.import.target).unwrap();
    assert_eq!(
        committed.transcript_workflow_authority().unwrap(),
        TranscriptWorkflowAuthorityState::Authoritative {
            source_generation: 10
        }
    );
}
