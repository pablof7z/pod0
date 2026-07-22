use pod0_domain::{CommandId, StateRevision};

use crate::download_store_test_support::DownloadFixture;
use crate::*;

#[test]
fn environment_and_waiting_intent_survive_restart_then_admit_once() {
    let fixture = DownloadFixture::new();
    let waiting = fixture.ensure(1, false);
    assert_eq!(waiting.stage, StoredDownloadStage::Waiting);
    assert_eq!(waiting.attempt, 0);
    assert!(
        fixture
            .store
            .pending_download_host_requests(20)
            .unwrap()
            .is_empty()
    );

    fixture
        .store
        .observe_download_environment(
            CommandId::from_parts(100, 2),
            &"e".repeat(64),
            StoredDownloadNetwork::Wifi,
            Some(2_000_000_000),
            1_800_000_000_200,
        )
        .unwrap();
    let reopened = LibraryStore::open_authoritative(&fixture.import.target).unwrap();
    assert_eq!(
        reopened.download_environment().unwrap(),
        DownloadEnvironmentRecord {
            network: StoredDownloadNetwork::Wifi,
            available_capacity_bytes: Some(2_000_000_000),
            observed_at_ms: 1_800_000_000_200,
        }
    );

    let transition = reopened
        .admit_waiting_download(
            fixture.episode_id,
            waiting.workflow_revision,
            StateRevision::new(9),
            1_800_000_000_201,
            1_800_086_400_201,
        )
        .unwrap();
    assert_eq!(transition.record.stage, StoredDownloadStage::Requested);
    assert_eq!(transition.record.attempt, 1);
    let requests = reopened.pending_download_host_requests(20).unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].request_id,
        transition.record.request_id.unwrap()
    );
    assert!(matches!(
        reopened.admit_waiting_download(
            fixture.episode_id,
            waiting.workflow_revision,
            StateRevision::new(9),
            1_800_000_000_202,
            1_800_086_400_202,
        ),
        Err(StorageError::DownloadWorkflowConflict)
    ));
}

#[test]
fn obsolete_waiting_intent_is_retired_without_creating_host_work() {
    let fixture = DownloadFixture::new();
    let waiting = fixture.ensure(1, false);
    let transition = fixture
        .store
        .retire_obsolete_waiting_download(
            fixture.episode_id,
            waiting.workflow_revision,
            1_800_000_000_250,
        )
        .unwrap();

    assert_eq!(transition.record.stage, StoredDownloadStage::Cancelled);
    assert_eq!(
        transition.record.desired_state,
        StoredDownloadDesiredState::Absent
    );
    assert!(
        fixture
            .store
            .pending_download_host_requests(20)
            .unwrap()
            .is_empty()
    );
    assert!(matches!(
        fixture.store.retire_obsolete_waiting_download(
            fixture.episode_id,
            waiting.workflow_revision,
            1_800_000_000_251,
        ),
        Err(StorageError::DownloadWorkflowConflict)
    ));
}

#[test]
fn command_identity_is_durable_and_conflicting_reuse_is_rejected() {
    let fixture = DownloadFixture::new();
    let first = fixture.ensure(1, true);
    let replay = fixture.ensure(1, true);
    assert_eq!(replay, first);
    assert_eq!(
        fixture
            .store
            .pending_download_host_requests(20)
            .unwrap()
            .len(),
        1
    );

    let episode = fixture.store.snapshot().unwrap().episodes[0].clone();
    let input_version = pod0_application::download_input_version(
        &episode.enclosure_url,
        episode.enclosure_mime_type.as_deref(),
        episode.duration_milliseconds,
    )
    .unwrap();
    let conflict = fixture.store.ensure_download_workflow(DownloadEnsureInput {
        episode_id: fixture.episode_id,
        intent_id: pod0_application::download_intent_id(fixture.episode_id, &input_version)
            .unwrap(),
        input_version,
        origin: StoredDownloadOrigin::Playback,
        admitted: true,
        wait_failure_code: None,
        command_id: CommandId::from_parts(100, 1),
        command_fingerprint: "f".repeat(64),
        cancellation_id: pod0_domain::CancellationId::from_parts(101, 1),
        enclosure_url: episode.enclosure_url,
        issued_revision: StateRevision::new(1),
        now_ms: 1_800_000_000_300,
        deadline_at_ms: 1_800_086_400_300,
    });
    assert_eq!(conflict.unwrap_err(), StorageError::DownloadCommandConflict);
}
