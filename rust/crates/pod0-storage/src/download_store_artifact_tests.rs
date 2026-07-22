use std::sync::atomic::{AtomicBool, Ordering};

use crate::download_store_test_support::{DownloadFixture, bytes_file};
use crate::*;

#[test]
fn accepted_task_and_staged_file_commit_one_canonical_artifact() {
    let fixture = DownloadFixture::new();
    let requested = fixture.ensure(1, true);
    let request_id = requested.request_id.unwrap();
    let accepted = fixture
        .store
        .accept_download_host_task(request_id, 1, "task-1", Some("resume-1"), 1_800_000_000_400)
        .unwrap();
    let DownloadObservationOutcome::Updated(accepted) = accepted else {
        panic!("expected accepted task")
    };
    assert_eq!(accepted.stage, StoredDownloadStage::HostAccepted);

    let payload = b"durable podcast media bytes";
    let path = bytes_file(&fixture, "native-staged.media", payload);
    let completed = fixture
        .store
        .complete_download_from_staged_file(
            request_id,
            2,
            &path,
            payload.len() as u64,
            1_800_000_000_401,
        )
        .unwrap();
    let DownloadObservationOutcome::Updated(completed) = completed else {
        panic!("expected completed artifact")
    };
    assert_eq!(completed.stage, StoredDownloadStage::Succeeded);
    let key = completed.artifact_key.as_deref().unwrap();
    assert_eq!(
        std::fs::read(fixture.store.download_artifact_path(key).unwrap()).unwrap(),
        payload
    );
    assert!(
        fixture
            .store
            .pending_download_host_requests(20)
            .unwrap()
            .is_empty()
    );
    let episode = fixture.store.snapshot().unwrap().episodes[0].clone();
    assert!(matches!(
        episode.download,
        pod0_domain::DownloadArtifactStatus::Available { byte_count, .. }
            if byte_count == payload.len() as u64
    ));

    assert!(matches!(
        fixture
            .store
            .complete_download_from_staged_file(
                request_id,
                2,
                &path,
                payload.len() as u64,
                1_800_000_000_402
            )
            .unwrap(),
        DownloadObservationOutcome::Duplicate(_)
    ));
}

struct InterruptOnce {
    boundary: DownloadArtifactBoundary,
    hit: AtomicBool,
}

impl InterruptOnce {
    fn at(boundary: DownloadArtifactBoundary) -> Self {
        Self {
            boundary,
            hit: AtomicBool::new(false),
        }
    }
}

impl DownloadArtifactObserver for InterruptOnce {
    fn reached(&self, boundary: DownloadArtifactBoundary) -> Result<(), StorageError> {
        if boundary == self.boundary && !self.hit.swap(true, Ordering::SeqCst) {
            Err(StorageError::Interrupted)
        } else {
            Ok(())
        }
    }
}

#[test]
fn recovery_adopts_after_each_filesystem_transaction_boundary() {
    for boundary in [
        DownloadArtifactBoundary::AfterStagedRecord,
        DownloadArtifactBoundary::AfterArtifactRename,
    ] {
        let fixture = DownloadFixture::new();
        let requested = fixture.ensure(1, true);
        let payload = b"restart-safe media";
        let path = bytes_file(&fixture, "interrupted.media", payload);
        let error = fixture
            .store
            .complete_download_with_observer(
                requested.request_id.unwrap(),
                1,
                &path,
                payload.len() as u64,
                1_800_000_000_500,
                &InterruptOnce::at(boundary),
            )
            .unwrap_err();
        assert_eq!(error, StorageError::Interrupted);

        let reopened = LibraryStore::open_authoritative(&fixture.import.target).unwrap();
        let report = reopened.recover_download_artifacts().unwrap();
        assert_eq!(report.adopted_count, 1);
        assert_eq!(report.repaired_count, 0);
        assert_eq!(
            reopened
                .download_workflow(fixture.episode_id)
                .unwrap()
                .unwrap()
                .stage,
            StoredDownloadStage::Succeeded
        );
        assert!(
            reopened
                .pending_download_host_requests(20)
                .unwrap()
                .is_empty()
        );
    }
}

#[test]
fn missing_staged_and_corrupt_canonical_files_become_typed_repair_state() {
    let missing = DownloadFixture::new();
    let requested = missing.ensure(1, true);
    let payload = b"will disappear";
    let path = bytes_file(&missing, "missing.media", payload);
    missing
        .store
        .complete_download_with_observer(
            requested.request_id.unwrap(),
            1,
            &path,
            payload.len() as u64,
            1_800_000_000_600,
            &InterruptOnce::at(DownloadArtifactBoundary::AfterStagedRecord),
        )
        .unwrap_err();
    let connection = rusqlite::Connection::open(&missing.import.target).unwrap();
    let staged: String = connection
        .query_row(
            "SELECT staged_path FROM pod0_download_attempts",
            [],
            |row| row.get(0),
        )
        .unwrap();
    std::fs::remove_file(staged).unwrap();
    let report = missing.store.recover_download_artifacts().unwrap();
    assert_eq!(report.repaired_count, 1);
    let repaired = missing
        .store
        .download_workflow(missing.episode_id)
        .unwrap()
        .unwrap();
    assert_eq!(repaired.stage, StoredDownloadStage::Failed);
    assert_eq!(repaired.failure_code.as_deref(), Some("invalid_artifact"));

    let staged_corrupt = DownloadFixture::new();
    let requested = staged_corrupt.ensure(1, true);
    let path = bytes_file(&staged_corrupt, "staged-corrupt.media", payload);
    staged_corrupt
        .store
        .complete_download_with_observer(
            requested.request_id.unwrap(),
            1,
            &path,
            payload.len() as u64,
            1_800_000_000_601,
            &InterruptOnce::at(DownloadArtifactBoundary::AfterStagedRecord),
        )
        .unwrap_err();
    let connection = rusqlite::Connection::open(&staged_corrupt.import.target).unwrap();
    let staged_path: String = connection
        .query_row(
            "SELECT staged_path FROM pod0_download_attempts",
            [],
            |row| row.get(0),
        )
        .unwrap();
    std::fs::write(&staged_path, b"corrupt").unwrap();
    assert_eq!(
        staged_corrupt
            .store
            .recover_download_artifacts()
            .unwrap()
            .repaired_count,
        1
    );
    assert!(!std::path::Path::new(&staged_path).exists());

    let corrupt = DownloadFixture::new();
    let requested = corrupt.ensure(1, true);
    let path = bytes_file(&corrupt, "valid.media", payload);
    let DownloadObservationOutcome::Updated(done) = corrupt
        .store
        .complete_download_from_staged_file(
            requested.request_id.unwrap(),
            1,
            &path,
            payload.len() as u64,
            1_800_000_000_602,
        )
        .unwrap()
    else {
        panic!("expected completion")
    };
    std::fs::write(
        corrupt
            .store
            .download_artifact_path(done.artifact_key.as_deref().unwrap())
            .unwrap(),
        b"corrupt",
    )
    .unwrap();
    assert_eq!(
        corrupt
            .store
            .recover_download_artifacts()
            .unwrap()
            .repaired_count,
        1
    );
    assert_eq!(
        corrupt
            .store
            .download_workflow(corrupt.episode_id)
            .unwrap()
            .unwrap()
            .stage,
        StoredDownloadStage::Failed
    );
    assert!(matches!(
        corrupt.store.snapshot().unwrap().episodes[0].download,
        pod0_domain::DownloadArtifactStatus::Unavailable
    ));
}
