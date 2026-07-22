use pod0_domain::{CommandId, StateRevision};

use crate::download_store_test_support::{DownloadFixture, bytes_file};
use crate::*;

#[test]
fn retry_cancel_and_remove_transitions_are_fenced_and_recoverable() {
    let retry = DownloadFixture::new();
    let requested = retry.ensure(1, true);
    let outcome = retry
        .store
        .fail_download_host_request(DownloadFailureInput {
            request_id: requested.request_id.unwrap(),
            sequence_number: 1,
            failure_code: "transport".to_owned(),
            failure_detail: None,
            retryable: true,
            retry_at_ms: Some(1_800_000_001_000),
            retry_deadline_at_ms: Some(1_800_086_401_000),
            issued_revision: StateRevision::new(8),
            observed_at_ms: 1_800_000_000_700,
        })
        .unwrap();
    let DownloadObservationOutcome::Updated(scheduled) = outcome else {
        panic!("expected retry")
    };
    assert_eq!(scheduled.stage, StoredDownloadStage::RetryScheduled);
    assert_eq!(scheduled.attempt, 2);
    assert_eq!(
        retry
            .store
            .pending_download_host_requests(20)
            .unwrap()
            .len(),
        1
    );

    let cancelled = retry
        .store
        .cancel_download_workflow(
            CommandId::from_parts(100, 9),
            &"c".repeat(64),
            retry.episode_id,
            scheduled.workflow_revision,
            StateRevision::new(9),
            1_800_000_000_701,
        )
        .unwrap()
        .record;
    assert_eq!(cancelled.stage, StoredDownloadStage::Cancelled);
    let cancellation = retry.store.pending_download_host_requests(20).unwrap();
    assert_eq!(cancellation.len(), 1);
    assert_eq!(cancellation[0].kind, DownloadHostRequestKind::Cancel);
    assert!(matches!(
        retry
            .store
            .complete_download_cancellation(cancellation[0].request_id, 1, 1_800_000_000_702)
            .unwrap(),
        DownloadObservationOutcome::Updated(_)
    ));

    let removal = DownloadFixture::new();
    let requested = removal.ensure(1, true);
    let payload = b"remove me";
    let path = bytes_file(&removal, "remove.media", payload);
    let DownloadObservationOutcome::Updated(done) = removal
        .store
        .complete_download_from_staged_file(
            requested.request_id.unwrap(),
            1,
            &path,
            payload.len() as u64,
            1_800_000_000_703,
        )
        .unwrap()
    else {
        panic!("expected completion")
    };
    let transition = removal
        .store
        .remove_download_artifact(DownloadRemovalInput {
            command_id: CommandId::from_parts(100, 10),
            command_fingerprint: "d".repeat(64),
            episode_id: removal.episode_id,
            expected_revision: done.workflow_revision,
            issued_revision: StateRevision::new(11),
            now_ms: 1_800_000_000_704,
            deadline_at_ms: 1_800_086_400_704,
        })
        .unwrap();
    assert_eq!(transition.record.stage, StoredDownloadStage::Removing);
    let request = removal
        .store
        .pending_download_host_requests(20)
        .unwrap()
        .pop()
        .unwrap();
    let key = request.artifact_key.clone().unwrap();
    std::fs::remove_file(removal.store.download_artifact_path(&key).unwrap()).unwrap();
    removal
        .store
        .complete_download_artifact_removal(request.request_id, 1, &key, 1_800_000_000_705)
        .unwrap();
    assert_eq!(
        removal
            .store
            .download_workflow(removal.episode_id)
            .unwrap()
            .unwrap()
            .stage,
        StoredDownloadStage::Cancelled
    );
    assert!(matches!(
        removal.store.snapshot().unwrap().episodes[0].download,
        pod0_domain::DownloadArtifactStatus::Unavailable
    ));
}
