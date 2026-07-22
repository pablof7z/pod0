use sha2::{Digest as _, Sha256};

use pod0_application::{
    TranscriptProvider, TranscriptWorkflowConfiguration, TranscriptWorkflowOrigin,
};

use crate::runtime_playback_test_support::PlaybackFixture;
use crate::*;

#[test]
fn legacy_workflow_cutover_survives_each_restart_and_recovers_owned_work() {
    let fixture = PlaybackFixture::new_before_transcript_workflow_cutover();
    let (row, candidate) = restart_candidate(&fixture);
    let backup_digest = digest(b"exact encrypted legacy workflow backup");

    let staged = fixture.facade.stage_legacy_transcript_workflow_cutover(
        backup_digest,
        row.row_bytes.len() as u64,
        vec![row.clone()],
        vec![candidate],
    );
    assert_eq!(staged.stage, LegacyTranscriptWorkflowCutoverStage::Staged);
    assert_eq!(staged.row_count, 1);
    assert_eq!(staged.adopted_workflow_count, 1);
    let generation = staged.source_generation.expect("source generation");

    let store = pod0_storage::LibraryStore::open_authoritative(&fixture.target).unwrap();
    let rollback = store.export_legacy_transcript_workflow_rollback().unwrap();
    assert_eq!(rollback.backup_digest, backup_digest);
    assert_eq!(rollback.backup_byte_count, row.row_bytes.len() as u64);
    assert_eq!(rollback.rows[0].row_bytes, row.row_bytes);

    let after_stage = reopen(&fixture);
    assert_eq!(
        after_stage.transcript_workflow_cutover().stage,
        LegacyTranscriptWorkflowCutoverStage::Staged
    );
    assert!(after_stage.next_host_requests(u16::MAX).is_empty());
    assert_eq!(
        after_stage
            .verify_legacy_transcript_workflow_cutover(generation)
            .stage,
        LegacyTranscriptWorkflowCutoverStage::Verified
    );

    let after_verify = reopen(&fixture);
    assert_eq!(
        after_verify.transcript_workflow_cutover().stage,
        LegacyTranscriptWorkflowCutoverStage::Verified
    );
    assert_eq!(
        after_verify
            .commit_legacy_transcript_workflow_cutover(generation)
            .stage,
        LegacyTranscriptWorkflowCutoverStage::Authoritative
    );

    let after_commit = reopen(&fixture);
    assert_eq!(
        after_commit.transcript_workflow_cutover().stage,
        LegacyTranscriptWorkflowCutoverStage::Authoritative
    );
    assert!(
        after_commit
            .next_host_requests(u16::MAX)
            .iter()
            .any(|request| {
                matches!(
                    request.request,
                    HostRequest::ExecuteTranscriptCapability { .. }
                )
            })
    );
}

#[test]
fn verified_cutover_can_be_discarded_without_granting_rust_authority() {
    let fixture = PlaybackFixture::new_before_transcript_workflow_cutover();
    let (row, candidate) = restart_candidate(&fixture);
    let staged = fixture.facade.stage_legacy_transcript_workflow_cutover(
        digest(b"rollback backup"),
        row.row_bytes.len() as u64,
        vec![row],
        vec![candidate],
    );
    let generation = staged.source_generation.unwrap();
    assert_eq!(
        fixture
            .facade
            .verify_legacy_transcript_workflow_cutover(generation)
            .stage,
        LegacyTranscriptWorkflowCutoverStage::Verified
    );
    assert_eq!(
        fixture
            .facade
            .discard_staged_legacy_transcript_workflow_cutover(generation)
            .stage,
        LegacyTranscriptWorkflowCutoverStage::NotStarted
    );

    let reopened = reopen(&fixture);
    assert_eq!(
        reopened.transcript_workflow_cutover().stage,
        LegacyTranscriptWorkflowCutoverStage::NotStarted
    );
    assert!(reopened.next_host_requests(u16::MAX).is_empty());
}

#[test]
fn invalid_legacy_rows_block_without_mutating_authority() {
    let fixture = PlaybackFixture::new_before_transcript_workflow_cutover();
    let (mut row, candidate) = restart_candidate(&fixture);
    row.row_fingerprint = ContentDigest::default();
    let blocked = fixture.facade.stage_legacy_transcript_workflow_cutover(
        digest(b"invalid backup"),
        row.row_bytes.len() as u64,
        vec![row],
        vec![candidate],
    );
    assert_eq!(blocked.stage, LegacyTranscriptWorkflowCutoverStage::Blocked);
    assert_eq!(
        fixture.facade.transcript_workflow_cutover().stage,
        LegacyTranscriptWorkflowCutoverStage::NotStarted
    );
}

#[test]
fn non_obsolete_row_cannot_be_silently_omitted_from_adoption() {
    let fixture = PlaybackFixture::new_before_transcript_workflow_cutover();
    let (row, _) = restart_candidate(&fixture);
    let blocked = fixture.facade.stage_legacy_transcript_workflow_cutover(
        digest(b"incomplete backup"),
        row.row_bytes.len() as u64,
        vec![row],
        Vec::new(),
    );
    assert_eq!(blocked.stage, LegacyTranscriptWorkflowCutoverStage::Blocked);
    assert_eq!(
        fixture.facade.transcript_workflow_cutover().stage,
        LegacyTranscriptWorkflowCutoverStage::NotStarted
    );
}

#[test]
fn automatic_publisher_restart_preserves_the_idempotent_fetch_path() {
    let fixture = PlaybackFixture::new_before_transcript_workflow_cutover();
    fixture.facade.state().listening.episodes[0]
        .feed_metadata
        .publisher_transcript = Some(pod0_domain::PublisherTranscriptReference {
        url: "https://legacy.example/transcript.vtt".into(),
        media_type: Some("text/vtt".into()),
        format: pod0_domain::PublisherTranscriptFormat::WebVtt,
    });
    let configuration = TranscriptWorkflowConfiguration {
        provider: TranscriptProvider::AssemblyAi,
        model: "universal-2".into(),
        local_audio_url: None,
        credential_available: false,
        auto_publisher_enabled: true,
        auto_provider_enabled: false,
    };
    let request = fixture
        .facade
        .state()
        .legacy_transcript_workflow_request(
            fixture.episode_id,
            TranscriptWorkflowOrigin::Automatic,
            configuration.clone(),
        )
        .unwrap()
        .0;
    assert!(request.publisher_first);
    let row_bytes = b"legacy publisher fetch".to_vec();
    let row = LegacyTranscriptWorkflowBackupRow {
        episode_id: fixture.episode_id,
        row_fingerprint: digest(&row_bytes),
        row_bytes,
        classification: LegacyTranscriptWorkflowRowClassification::Restart,
    };
    let staged = fixture.facade.stage_legacy_transcript_workflow_cutover(
        digest(b"publisher backup"),
        row.row_bytes.len() as u64,
        vec![row],
        vec![LegacyTranscriptWorkflowCutoverCandidate {
            episode_id: fixture.episode_id,
            source_revision: request.source_revision,
            origin: TranscriptWorkflowOrigin::Automatic,
            configuration,
            disposition: LegacyTranscriptWorkflowCutoverDisposition::Restart { attempt: 1 },
        }],
    );
    let generation = staged.source_generation.unwrap();
    assert_eq!(
        fixture
            .facade
            .verify_legacy_transcript_workflow_cutover(generation)
            .stage,
        LegacyTranscriptWorkflowCutoverStage::Verified
    );
    assert_eq!(
        fixture
            .facade
            .commit_legacy_transcript_workflow_cutover(generation)
            .stage,
        LegacyTranscriptWorkflowCutoverStage::Authoritative
    );
    let request = fixture
        .facade
        .next_host_requests(u16::MAX)
        .into_iter()
        .find(|request| {
            matches!(
                request.request,
                HostRequest::ExecuteTranscriptCapability { .. }
            )
        })
        .expect("publisher fetch request");
    assert!(matches!(
        request.request,
        HostRequest::ExecuteTranscriptCapability {
            capability: pod0_application::TranscriptCapabilityRequest::FetchPublisher { .. }
        }
    ));
}

fn restart_candidate(
    fixture: &PlaybackFixture,
) -> (
    LegacyTranscriptWorkflowBackupRow,
    LegacyTranscriptWorkflowCutoverCandidate,
) {
    let configuration = TranscriptWorkflowConfiguration {
        provider: TranscriptProvider::AssemblyAi,
        model: "universal-2".into(),
        local_audio_url: None,
        credential_available: false,
        auto_publisher_enabled: false,
        auto_provider_enabled: false,
    };
    let request = fixture
        .facade
        .state()
        .legacy_transcript_workflow_request(
            fixture.episode_id,
            TranscriptWorkflowOrigin::User,
            configuration.clone(),
        )
        .expect("stable migration request")
        .0;
    let row_bytes = br#"{"legacy":"transcript-ingest","state":"retry"}"#.to_vec();
    let row = LegacyTranscriptWorkflowBackupRow {
        episode_id: fixture.episode_id,
        row_fingerprint: digest(&row_bytes),
        row_bytes,
        classification: LegacyTranscriptWorkflowRowClassification::Restart,
    };
    let candidate = LegacyTranscriptWorkflowCutoverCandidate {
        episode_id: fixture.episode_id,
        source_revision: request.source_revision,
        origin: TranscriptWorkflowOrigin::User,
        configuration,
        disposition: LegacyTranscriptWorkflowCutoverDisposition::Restart { attempt: 1 },
    };
    (row, candidate)
}

fn reopen(fixture: &PlaybackFixture) -> std::sync::Arc<Pod0Facade> {
    Pod0Facade::open(fixture.target.to_string_lossy().into_owned()).unwrap()
}

fn digest(value: &[u8]) -> ContentDigest {
    ContentDigest::from_bytes(Sha256::digest(value).into())
}
