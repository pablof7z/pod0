use tempfile::tempdir;

use crate::{RecallIndex, RecallIndexError};

#[test]
fn legacy_cutover_marker_accepts_only_a_bounded_native_deletion_receipt() {
    let directory = tempdir().unwrap();
    let current = directory.path().join("rust.sqlite");
    let index = RecallIndex::open(&current, 4).unwrap();
    assert!(matches!(
        index.commit_legacy_cutover(4),
        Err(RecallIndexError::InvalidInput(_))
    ));
    assert!(!index.legacy_cutover_is_committed().unwrap());

    let receipt = index.commit_legacy_cutover(2).unwrap();

    assert_eq!(receipt.schema_version, 1);
    assert_eq!(receipt.removed_legacy_file_count, 2);
    assert!(index.legacy_cutover_is_committed().unwrap());
    assert!(current.exists());
}

#[test]
fn invalid_disposable_artifact_is_preserved_during_recovery_cleanup() {
    let directory = tempdir().unwrap();
    let legacy = directory.path().join("vectors.sqlite");
    std::fs::create_dir(&legacy).unwrap();

    assert!(crate::migration::remove_disposable_artifacts(&legacy).is_err());

    assert!(legacy.is_dir());
}
