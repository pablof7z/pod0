use pod0_application::{
    TranscriptProvider, transcript_attempt_id, transcript_submission_fence_id,
    transcript_workflow_id,
};
use pod0_domain::{CancellationId, CommandId, ContentDigest, HostRequestId, StateRevision};
use sha2::{Digest as _, Sha256};

use super::*;
use crate::transcript_store_test_support::{TranscriptFixture, input as artifact_input};
use crate::{LibraryStore, StorageError};

const NOW: i64 = 1_800_000_200_000;

#[test]
fn staged_import_preserves_exact_rows_and_recovery_identity() {
    let fixture = TranscriptFixture::new();
    let store = LibraryStore::open_authoritative(&fixture.import.target).unwrap();
    let episode_id = store.snapshot().unwrap().episodes[0].episode_id;
    let request = request(episode_id);
    let attempt_id = transcript_attempt_id(request.workflow_id, 2).unwrap();
    let row = backup_row(
        episode_id,
        br#"{"id":"legacy-transcript-job","state":"running","attempt":2}"#,
        LegacyTranscriptWorkflowRowClassification::RecoverProvider,
    );
    let rows = vec![row.clone()];
    let source_fingerprint = transcript_workflow_source_fingerprint(&rows);
    let input = LegacyTranscriptWorkflowCutoverInput {
        source_generation: 7,
        source_fingerprint,
        backup_digest: digest(b"exact-backup-file"),
        backup_byte_count: 4_096,
        rows,
        candidates: vec![LegacyTranscriptWorkflowCandidate {
            episode_id,
            request,
            request_id: Some(HostRequestId::from_parts(7, 1)),
            prepared_attempt: Some(PreparedTranscriptAttempt {
                attempt: 2,
                attempt_id,
                submission_fence_id: transcript_submission_fence_id(attempt_id),
            }),
            deadline_at_ms: None,
            expected_selection_revision: StateRevision::INITIAL,
            disposition: LegacyTranscriptWorkflowDisposition::RecoverProvider {
                external_operation_id: "legacy-provider-operation".to_owned(),
                provider_status: Some("processing".to_owned()),
            },
        }],
        command_id: CommandId::from_parts(7, 2),
        cancellation_id: CancellationId::from_parts(7, 3),
        issued_revision: StateRevision::new(7),
        max_attempts: 8,
        now_ms: NOW,
    };

    let staged = store
        .stage_legacy_transcript_workflow_cutover(input.clone())
        .unwrap();
    assert_eq!(
        staged.state,
        TranscriptWorkflowAuthorityState::Staged {
            source_generation: 7
        }
    );
    assert_eq!(staged.row_count, 1);
    assert_eq!(staged.adopted_workflow_count, 1);
    let export = store.export_legacy_transcript_workflow_rollback().unwrap();
    assert_eq!(export.rows, [row]);
    assert_eq!(export.source_fingerprint, source_fingerprint);

    assert_eq!(
        store.verify_legacy_transcript_workflow_cutover(
            7,
            ContentDigest::from_bytes([0xFF; 32]),
            NOW + 1,
        ),
        Err(StorageError::TranscriptWorkflowConflict)
    );
    store
        .verify_legacy_transcript_workflow_cutover(7, source_fingerprint, NOW + 1)
        .unwrap();
    assert_eq!(
        store.commit_legacy_transcript_workflow_cutover(
            7,
            ContentDigest::from_bytes([0xEE; 32]),
            NOW + 2,
        ),
        Err(StorageError::TranscriptWorkflowConflict)
    );
    store
        .commit_legacy_transcript_workflow_cutover(7, source_fingerprint, NOW + 2)
        .unwrap();

    let reopened = LibraryStore::open_authoritative(&fixture.import.target).unwrap();
    let workflow = reopened.transcript_workflow(episode_id).unwrap().unwrap();
    assert_eq!(
        workflow.stage,
        StoredTranscriptWorkflowStage::ProviderAccepted
    );
    assert_eq!(
        workflow.external_operation_id.as_deref(),
        Some("legacy-provider-operation")
    );
    assert_eq!(
        reopened
            .recover_transcript_workflows(NOW + 3, 10)
            .unwrap()
            .provider_recoveries,
        [workflow.request_id.unwrap()]
    );
    assert_eq!(
        reopened.export_legacy_transcript_workflow_rollback(),
        Err(StorageError::TranscriptWorkflowConflict)
    );
}

#[test]
fn verification_failure_can_discard_without_activating_authority() {
    let fixture = TranscriptFixture::new();
    let store = LibraryStore::open_authoritative(&fixture.import.target).unwrap();
    let episode_id = store.snapshot().unwrap().episodes[0].episode_id;
    let row = backup_row(
        episode_id,
        b"legacy-obsolete-row",
        LegacyTranscriptWorkflowRowClassification::Obsolete,
    );
    let rows = vec![row];
    let source_fingerprint = transcript_workflow_source_fingerprint(&rows);
    store
        .stage_legacy_transcript_workflow_cutover(LegacyTranscriptWorkflowCutoverInput {
            source_generation: 8,
            source_fingerprint,
            backup_digest: digest(b"backup"),
            backup_byte_count: 1_024,
            rows,
            candidates: Vec::new(),
            command_id: CommandId::from_parts(8, 1),
            cancellation_id: CancellationId::from_parts(8, 2),
            issued_revision: StateRevision::new(8),
            max_attempts: 8,
            now_ms: NOW,
        })
        .unwrap();
    assert!(store.discard_legacy_transcript_workflow_cutover(8).unwrap());
    assert_eq!(
        store.transcript_workflow_authority().unwrap(),
        TranscriptWorkflowAuthorityState::NotStarted
    );
    assert_eq!(
        store.export_legacy_transcript_workflow_rollback(),
        Err(StorageError::TranscriptWorkflowConflict)
    );
}

#[test]
fn selected_transcript_and_pending_index_are_adopted_without_dual_write() {
    let fixture = TranscriptFixture::new();
    let artifact = artifact_input("audio-v1");
    let committed = fixture
        .store
        .commit_and_select(
            CommandId::from_parts(9, 1),
            StateRevision::INITIAL,
            artifact,
            NOW,
        )
        .unwrap();
    let store = LibraryStore::open_authoritative(&fixture.import.target).unwrap();
    let episode_id = store.snapshot().unwrap().episodes[0].episode_id;
    let row = backup_row(
        episode_id,
        b"legacy-index-pending-row",
        LegacyTranscriptWorkflowRowClassification::IndexPending,
    );
    let rows = vec![row];
    let source_fingerprint = transcript_workflow_source_fingerprint(&rows);
    store
        .stage_legacy_transcript_workflow_cutover(LegacyTranscriptWorkflowCutoverInput {
            source_generation: 9,
            source_fingerprint,
            backup_digest: digest(b"backup-index"),
            backup_byte_count: 2_048,
            rows,
            candidates: vec![LegacyTranscriptWorkflowCandidate {
                episode_id,
                request: request(episode_id),
                request_id: None,
                prepared_attempt: None,
                deadline_at_ms: None,
                expected_selection_revision: StateRevision::INITIAL,
                disposition: LegacyTranscriptWorkflowDisposition::IndexPending {
                    artifact_id: committed.artifact_id,
                    transcript_version_id: committed.transcript_version_id,
                    content_digest: committed.transcript_content_digest,
                    selection_revision: committed.selection_revision,
                    evidence_input_version: "legacy-evidence-v1".to_owned(),
                },
            }],
            command_id: CommandId::from_parts(9, 2),
            cancellation_id: CancellationId::from_parts(9, 3),
            issued_revision: StateRevision::new(9),
            max_attempts: 8,
            now_ms: NOW + 1,
        })
        .unwrap();
    store
        .verify_legacy_transcript_workflow_cutover(9, source_fingerprint, NOW + 2)
        .unwrap();
    store
        .commit_legacy_transcript_workflow_cutover(9, source_fingerprint, NOW + 3)
        .unwrap();
    let workflow = store.transcript_workflow(episode_id).unwrap().unwrap();
    assert_eq!(
        workflow.stage,
        StoredTranscriptWorkflowStage::EvidenceRequested
    );
    assert_eq!(workflow.committed_artifact_id, Some(committed.artifact_id));
    assert_eq!(
        store
            .recover_transcript_workflows(NOW + 4, 10)
            .unwrap()
            .evidence_requests,
        [workflow.request.workflow_id]
    );
}

fn request(episode_id: pod0_domain::EpisodeId) -> StoredTranscriptWorkflowRequest {
    let source_revision = "audio-v1".to_owned();
    let model = "universal-3-pro".to_owned();
    StoredTranscriptWorkflowRequest {
        workflow_id: transcript_workflow_id(
            episode_id,
            &source_revision,
            TranscriptProvider::AssemblyAi,
            &model,
        ),
        source_revision,
        origin: "user".to_owned(),
        provider: "assembly-ai".to_owned(),
        model,
        remote_audio_url: "https://example.test/episode.mp3".to_owned(),
        local_audio_url: None,
        publisher_transcript_url: None,
        publisher_mime_hint: None,
        publisher_first: false,
        provider_fallback_enabled: true,
    }
}

fn backup_row(
    episode_id: pod0_domain::EpisodeId,
    bytes: &[u8],
    classification: LegacyTranscriptWorkflowRowClassification,
) -> LegacyTranscriptWorkflowBackupRow {
    LegacyTranscriptWorkflowBackupRow {
        episode_id,
        row_bytes: bytes.to_vec(),
        row_fingerprint: digest(bytes),
        classification,
    }
}

fn digest(bytes: &[u8]) -> ContentDigest {
    let mut hash = Sha256::new();
    hash.update(bytes);
    ContentDigest::from_bytes(hash.finalize().into())
}
