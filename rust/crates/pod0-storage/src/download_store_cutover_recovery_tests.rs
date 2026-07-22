use pod0_application::{download_attempt_id, download_input_version, download_intent_id};
use pod0_domain::{CancellationId, CommandId, StateRevision};

use crate::download_store_test_support::{DownloadFixture, bytes_file};
use crate::*;

fn input(
    fixture: &DownloadFixture,
    generation: u64,
    disposition: LegacyDownloadCutoverDisposition,
) -> LegacyDownloadCutoverInput {
    let episode = fixture.store.snapshot().unwrap().episodes[0].clone();
    let version = download_input_version(
        &episode.enclosure_url,
        episode.enclosure_mime_type.as_deref(),
        episode.duration_milliseconds,
    )
    .unwrap();
    let intent = download_intent_id(episode.episode_id, &version).unwrap();
    let attempt = download_attempt_id(intent, 1).unwrap();
    LegacyDownloadCutoverInput {
        source_generation: generation,
        entries: vec![LegacyDownloadCutoverEntry {
            episode_id: episode.episode_id,
            intent_id: intent,
            attempt_id: attempt,
            request_id: download_start_request_id(attempt),
            input_version: version,
            enclosure_url: episode.enclosure_url,
            origin: StoredDownloadOrigin::User,
            command_id: CommandId::from_parts(generation, 1),
            cancellation_id: CancellationId::from_parts(generation, 2),
            disposition,
        }],
        issued_revision: StateRevision::new(9),
        now_ms: 1_800_000_000_100,
        deadline_at_ms: 1_800_086_400_100,
    }
}

#[test]
fn staged_cutover_can_roll_back_exactly_but_authoritative_state_cannot() {
    let fixture = DownloadFixture::new_before_download_cutover();
    let bytes = b"rollback legacy download";
    let path = bytes_file(&fixture, "rollback.mp3", bytes);
    let source = input(
        &fixture,
        13,
        LegacyDownloadCutoverDisposition::Available {
            source_path: path,
            byte_count: bytes.len() as u64,
        },
    );
    fixture
        .store
        .stage_legacy_download_cutover(source.clone())
        .unwrap();
    let workflow = fixture
        .store
        .download_workflow(fixture.episode_id)
        .unwrap()
        .unwrap();
    let artifact = fixture
        .store
        .download_artifact_path(workflow.artifact_key.as_deref().unwrap())
        .unwrap();
    assert!(artifact.exists());
    assert_eq!(
        fixture
            .store
            .discard_staged_legacy_download_cutover(source.clone())
            .unwrap(),
        DownloadWorkflowAuthorityState::NotStarted
    );
    assert!(!artifact.exists());
    assert!(
        fixture
            .store
            .download_workflow(fixture.episode_id)
            .unwrap()
            .is_none()
    );
    assert!(matches!(
        fixture.store.snapshot().unwrap().episodes[0].download,
        pod0_domain::DownloadArtifactStatus::Available { byte_count, .. }
            if byte_count == bytes.len() as u64
    ));

    fixture
        .store
        .stage_legacy_download_cutover(source.clone())
        .unwrap();
    fixture
        .store
        .commit_legacy_download_cutover(13, 1_800_000_000_102)
        .unwrap();
    assert_eq!(
        fixture
            .store
            .discard_staged_legacy_download_cutover(source)
            .unwrap_err(),
        StorageError::DownloadWorkflowConflict
    );
}

#[test]
fn staged_artifact_recovers_after_process_restart_before_commit() {
    let fixture = DownloadFixture::new_before_download_cutover();
    let bytes = b"interrupted legacy cutover";
    let path = bytes_file(&fixture, "interrupted.mp3", bytes);
    let source = input(
        &fixture,
        14,
        LegacyDownloadCutoverDisposition::Available {
            source_path: path,
            byte_count: bytes.len() as u64,
        },
    );
    fixture
        .store
        .stage_legacy_download_cutover(source.clone())
        .unwrap();

    let reopened = LibraryStore::open_authoritative(&fixture.import.target).unwrap();
    let resumed = reopened.stage_legacy_download_cutover(source).unwrap();
    assert_eq!(resumed.adopted_available, 1);
    assert_eq!(resumed.scheduled_restart, 0);
    assert_eq!(
        reopened
            .commit_legacy_download_cutover(14, 1_800_000_000_102)
            .unwrap(),
        DownloadWorkflowAuthorityState::Authoritative {
            source_generation: 14
        }
    );
    assert!(matches!(
        reopened.snapshot().unwrap().episodes[0].download,
        pod0_domain::DownloadArtifactStatus::Available { byte_count, .. }
            if byte_count == bytes.len() as u64
    ));
}

#[test]
fn byte_count_mismatch_is_repaired_as_exactly_one_restart() {
    let fixture = DownloadFixture::new_before_download_cutover();
    let bytes = b"truncated legacy audio";
    let path = bytes_file(&fixture, "truncated.mp3", bytes);
    let staged = fixture
        .store
        .stage_legacy_download_cutover(input(
            &fixture,
            15,
            LegacyDownloadCutoverDisposition::Available {
                source_path: path,
                byte_count: bytes.len() as u64 + 1,
            },
        ))
        .unwrap();
    assert_eq!(staged.adopted_available, 0);
    assert_eq!(staged.scheduled_restart, 1);
    assert_eq!(staged.repaired_invalid, 1);
    fixture
        .store
        .commit_legacy_download_cutover(15, 1_800_000_000_102)
        .unwrap();
    assert_eq!(
        fixture
            .store
            .pending_download_host_requests(20)
            .unwrap()
            .len(),
        1
    );
}
