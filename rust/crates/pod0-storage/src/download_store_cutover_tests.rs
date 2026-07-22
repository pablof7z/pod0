use pod0_application::{download_attempt_id, download_input_version, download_intent_id};
use pod0_domain::{CancellationId, CommandId, StateRevision};

use crate::download_store_test_support::{DownloadFixture, bytes_file};
use crate::*;

fn input(
    fixture: &DownloadFixture,
    generation: u64,
    dispositions: Vec<LegacyDownloadCutoverDisposition>,
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
    let entries = dispositions
        .into_iter()
        .enumerate()
        .map(|(index, disposition)| LegacyDownloadCutoverEntry {
            episode_id: episode.episode_id,
            intent_id: intent,
            attempt_id: attempt,
            request_id: download_start_request_id(attempt),
            input_version: version.clone(),
            enclosure_url: episode.enclosure_url.clone(),
            origin: StoredDownloadOrigin::User,
            command_id: CommandId::from_parts(generation, index as u64 + 1),
            cancellation_id: CancellationId::from_parts(generation, index as u64 + 2),
            disposition,
        })
        .collect();
    LegacyDownloadCutoverInput {
        source_generation: generation,
        entries,
        issued_revision: StateRevision::new(9),
        now_ms: 1_800_000_000_100,
        deadline_at_ms: 1_800_086_400_100,
    }
}

#[test]
fn empty_cutover_is_restart_safe_and_authoritative_once() {
    let fixture = DownloadFixture::new_before_download_cutover();
    let staged = fixture
        .store
        .stage_legacy_download_cutover(input(&fixture, 7, Vec::new()))
        .unwrap();
    assert_eq!(
        staged.state,
        DownloadWorkflowAuthorityState::Staged {
            source_generation: 7
        }
    );
    assert!(
        fixture
            .store
            .pending_download_host_requests(20)
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        fixture
            .store
            .stage_legacy_download_cutover(input(&fixture, 7, Vec::new()))
            .unwrap(),
        staged
    );
    assert_eq!(
        fixture
            .store
            .commit_legacy_download_cutover(7, 1_800_000_000_101)
            .unwrap(),
        DownloadWorkflowAuthorityState::Authoritative {
            source_generation: 7
        }
    );
    assert_eq!(
        fixture
            .store
            .commit_legacy_download_cutover(7, 1_800_000_000_102)
            .unwrap(),
        DownloadWorkflowAuthorityState::Authoritative {
            source_generation: 7
        }
    );
}

#[test]
fn verified_legacy_audio_is_copied_and_selected_before_commit() {
    let fixture = DownloadFixture::new_before_download_cutover();
    let bytes = b"legacy offline episode";
    let path = bytes_file(&fixture, "legacy-offline.mp3", bytes);
    let staged = fixture
        .store
        .stage_legacy_download_cutover(input(
            &fixture,
            8,
            vec![LegacyDownloadCutoverDisposition::Available {
                source_path: path.clone(),
                byte_count: bytes.len() as u64,
            }],
        ))
        .unwrap();
    assert_eq!(staged.adopted_available, 1);
    assert_eq!(staged.scheduled_restart, 0);
    assert!(
        fixture
            .store
            .pending_download_host_requests(20)
            .unwrap()
            .is_empty()
    );
    let workflow = fixture
        .store
        .download_workflow(fixture.episode_id)
        .unwrap()
        .unwrap();
    assert_eq!(workflow.stage, StoredDownloadStage::Succeeded);
    assert_eq!(
        std::fs::read(
            fixture
                .store
                .download_artifact_path(workflow.artifact_key.as_deref().unwrap())
                .unwrap()
        )
        .unwrap(),
        bytes
    );
    assert_eq!(std::fs::read(path).unwrap(), bytes);

    fixture
        .store
        .commit_legacy_download_cutover(8, 1_800_000_000_101)
        .unwrap();
    assert!(matches!(
        fixture.store.snapshot().unwrap().episodes[0].download,
        pod0_domain::DownloadArtifactStatus::Available { byte_count, .. }
            if byte_count == bytes.len() as u64
    ));
}

#[test]
fn missing_legacy_audio_becomes_one_restart_request_after_commit() {
    let fixture = DownloadFixture::new_before_download_cutover();
    let missing = fixture
        .import
        ._directory
        .path()
        .join("missing.mp3")
        .to_string_lossy()
        .into_owned();
    let staged = fixture
        .store
        .stage_legacy_download_cutover(input(
            &fixture,
            9,
            vec![LegacyDownloadCutoverDisposition::Available {
                source_path: missing,
                byte_count: 45,
            }],
        ))
        .unwrap();
    assert_eq!(staged.repaired_invalid, 1);
    assert_eq!(staged.scheduled_restart, 1);
    assert!(
        fixture
            .store
            .pending_download_host_requests(20)
            .unwrap()
            .is_empty()
    );
    fixture
        .store
        .commit_legacy_download_cutover(9, 1_800_000_000_101)
        .unwrap();
    let requests = fixture.store.pending_download_host_requests(20).unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].episode_id, fixture.episode_id);
    let reopened = LibraryStore::open_authoritative(&fixture.import.target).unwrap();
    assert_eq!(
        reopened.pending_download_host_requests(20).unwrap().len(),
        1
    );
    assert!(matches!(
        reopened.snapshot().unwrap().episodes[0].download,
        pod0_domain::DownloadArtifactStatus::Unavailable
    ));
}

#[test]
fn resume_key_is_preserved_but_duplicates_and_preexisting_core_state_fail_closed() {
    let fixture = DownloadFixture::new_before_download_cutover();
    fixture
        .store
        .stage_legacy_download_cutover(input(
            &fixture,
            10,
            vec![LegacyDownloadCutoverDisposition::Restart {
                resume_available: true,
            }],
        ))
        .unwrap();
    fixture
        .store
        .commit_legacy_download_cutover(10, 1_800_000_000_101)
        .unwrap();
    let attempt = fixture
        .store
        .download_workflow(fixture.episode_id)
        .unwrap()
        .unwrap()
        .attempt_id
        .unwrap();
    let resume = format!("v1/{}.resume", hex(&attempt.into_bytes()));
    assert_eq!(
        fixture.store.pending_download_host_requests(20).unwrap()[0]
            .resume_key
            .as_deref(),
        Some(resume.as_str())
    );

    let duplicate = DownloadFixture::new_before_download_cutover();
    let mut duplicate_input = input(
        &duplicate,
        11,
        vec![LegacyDownloadCutoverDisposition::Restart {
            resume_available: false,
        }],
    );
    duplicate_input
        .entries
        .push(duplicate_input.entries[0].clone());
    assert_eq!(
        duplicate
            .store
            .stage_legacy_download_cutover(duplicate_input)
            .unwrap_err(),
        StorageError::DownloadWorkflowConflict
    );
    assert_eq!(
        duplicate.store.download_workflow_authority().unwrap(),
        DownloadWorkflowAuthorityState::NotStarted
    );

    let conflict = DownloadFixture::new_before_download_cutover();
    conflict.ensure(1, true);
    assert_eq!(
        conflict
            .store
            .stage_legacy_download_cutover(input(&conflict, 12, Vec::new()))
            .unwrap_err(),
        StorageError::DownloadWorkflowConflict
    );
}

fn hex(value: &[u8]) -> String {
    value.iter().map(|byte| format!("{byte:02x}")).collect()
}
