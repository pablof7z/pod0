use pod0_domain::{CancellationId, CommandId, ContentDigest, HostRequestId, StateRevision};
use sha2::{Digest as _, Sha256};

use super::test_support::{Fixture, NOW, changed};
use super::*;
use crate::LibraryStore;
use crate::transcript_store_test_support::{TranscriptFixture, input as artifact_input};

#[test]
fn prerequisite_and_publisher_stages_survive_restart() {
    let awaiting_fixture = Fixture::new();
    let awaiting = changed(
        awaiting_fixture
            .store
            .ensure_transcript_workflow(TranscriptWorkflowEnsureInput {
                episode_id: awaiting_fixture.episode_id,
                request: awaiting_fixture.request(false),
                stage: StoredTranscriptWorkflowStage::AwaitingPrerequisite,
                prepared_attempt: None,
                command_id: CommandId::from_parts(95, 1),
                cancellation_id: CancellationId::from_parts(95, 2),
                request_id: None,
                issued_revision: StateRevision::INITIAL,
                deadline_at_ms: None,
                expected_selection_revision: StateRevision::INITIAL,
                max_attempts: 8,
                now_ms: NOW + 10,
                expected_workflow_revision: None,
            })
            .unwrap(),
    );
    assert_eq!(
        awaiting_fixture
            .reopen()
            .transcript_workflow(awaiting_fixture.episode_id)
            .unwrap(),
        Some(awaiting)
    );

    let publisher_fixture = Fixture::new();
    let request_id = HostRequestId::from_parts(96, 1);
    let publisher = changed(
        publisher_fixture
            .store
            .ensure_transcript_workflow(TranscriptWorkflowEnsureInput {
                episode_id: publisher_fixture.episode_id,
                request: publisher_fixture.request(true),
                stage: StoredTranscriptWorkflowStage::PublisherRequested,
                prepared_attempt: None,
                command_id: CommandId::from_parts(96, 2),
                cancellation_id: CancellationId::from_parts(96, 3),
                request_id: Some(request_id),
                issued_revision: StateRevision::new(2),
                deadline_at_ms: Some(NOW + 60_000),
                expected_selection_revision: StateRevision::INITIAL,
                max_attempts: 8,
                now_ms: NOW + 10,
                expected_workflow_revision: None,
            })
            .unwrap(),
    );
    assert_eq!(
        publisher.stage,
        StoredTranscriptWorkflowStage::PublisherRequested
    );
    assert_eq!(
        publisher_fixture
            .reopen()
            .recover_transcript_workflows(NOW + 11, 10)
            .unwrap()
            .dispatchable_requests,
        [request_id]
    );
}

#[test]
fn transcript_committed_stage_recovers_evidence_admission_after_restart() {
    let fixture = TranscriptFixture::new();
    let committed = fixture
        .store
        .commit_and_select(
            CommandId::from_parts(97, 1),
            StateRevision::INITIAL,
            artifact_input("audio-v1"),
            NOW,
        )
        .unwrap();
    let store = LibraryStore::open_authoritative(&fixture.import.target).unwrap();
    let episode_id = store.snapshot().unwrap().episodes[0].episode_id;
    let request = request(episode_id);
    let bytes = b"legacy-transcript-committed";
    let row = LegacyTranscriptWorkflowBackupRow {
        episode_id,
        row_bytes: bytes.to_vec(),
        row_fingerprint: digest(bytes),
        classification: LegacyTranscriptWorkflowRowClassification::Succeeded,
    };
    let rows = vec![row];
    let fingerprint = transcript_workflow_source_fingerprint(&rows);
    store
        .stage_legacy_transcript_workflow_cutover(LegacyTranscriptWorkflowCutoverInput {
            source_generation: 11,
            source_fingerprint: fingerprint,
            backup_digest: digest(b"backup"),
            backup_byte_count: bytes.len() as u64,
            rows,
            candidates: vec![LegacyTranscriptWorkflowCandidate {
                episode_id,
                request: request.clone(),
                request_id: None,
                prepared_attempt: None,
                deadline_at_ms: None,
                expected_selection_revision: StateRevision::INITIAL,
                disposition: LegacyTranscriptWorkflowDisposition::Succeeded {
                    artifact_id: committed.artifact_id,
                    transcript_version_id: committed.transcript_version_id,
                    content_digest: committed.transcript_content_digest,
                    selection_revision: committed.selection_revision,
                },
            }],
            command_id: CommandId::from_parts(97, 2),
            cancellation_id: CancellationId::from_parts(97, 3),
            issued_revision: StateRevision::new(11),
            max_attempts: 8,
            now_ms: NOW + 1,
        })
        .unwrap();
    store
        .verify_legacy_transcript_workflow_cutover(11, fingerprint, NOW + 2)
        .unwrap();
    store
        .commit_legacy_transcript_workflow_cutover(11, fingerprint, NOW + 3)
        .unwrap();

    let reopened = LibraryStore::open_authoritative(&fixture.import.target).unwrap();
    assert_eq!(
        reopened
            .transcript_workflow(episode_id)
            .unwrap()
            .unwrap()
            .stage,
        StoredTranscriptWorkflowStage::TranscriptCommitted
    );
    assert_eq!(
        reopened
            .recover_transcript_workflows(NOW + 4, 10)
            .unwrap()
            .evidence_requests,
        [request.workflow_id]
    );
}

fn request(episode_id: pod0_domain::EpisodeId) -> StoredTranscriptWorkflowRequest {
    let source_revision = "audio-v1".to_owned();
    StoredTranscriptWorkflowRequest {
        workflow_id: pod0_application::transcript_workflow_id(
            episode_id,
            &source_revision,
            pod0_application::TranscriptProvider::AssemblyAi,
            "universal-3-pro",
        ),
        source_revision,
        origin: "user".to_owned(),
        provider: "assembly-ai".to_owned(),
        model: "universal-3-pro".to_owned(),
        remote_audio_url: "https://example.test/episode.mp3".to_owned(),
        local_audio_url: None,
        publisher_transcript_url: None,
        publisher_mime_hint: None,
        publisher_first: false,
        provider_fallback_enabled: true,
    }
}

fn digest(bytes: &[u8]) -> ContentDigest {
    let mut hash = Sha256::new();
    hash.update(bytes);
    ContentDigest::from_bytes(hash.finalize().into())
}
