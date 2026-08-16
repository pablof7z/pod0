use crate::{StorageError, UserDataTarget, UserDataTargetKind, ValidatedUserDataInventory};

#[test]
fn marker_wire_is_stable_and_unknown_values_fail_closed() {
    use crate::user_data_erasure_marker::TargetState;
    for kind in UserDataTargetKind::ALL {
        let encoded = serde_json::to_string(&kind).unwrap();
        assert_eq!(encoded, format!(r#""{}""#, kind.wire_name()));
        assert_eq!(serde_json::from_str::<UserDataTargetKind>(&encoded).unwrap(), kind);
    }
    assert!(serde_json::from_str::<UserDataTargetKind>(r#""future_kind""#).is_err());
    assert_eq!(serde_json::to_string(&TargetState::Covered).unwrap(), r#""covered""#);
    assert!(serde_json::from_str::<TargetState>(r#""future_state""#).is_err());
}

#[test]
fn inventory_is_exhaustive_ordered_and_accepts_absent_exact_targets() {
    let directory = tempfile::tempdir().unwrap();
    let unordered_targets = targets(directory.path());
    let inventory = ValidatedUserDataInventory::validate(
        unordered_targets.into_iter().rev().collect(),
        &[directory.path().to_path_buf()],
    ).unwrap();
    assert_eq!(inventory.targets().len(), UserDataTargetKind::ALL.len());
    assert_eq!(inventory.targets()[0].kind, UserDataTargetKind::CoreSqlite);
    assert!(crate::UserDataErasureExclusion::ALL
        .iter()
        .all(|exclusion| !exclusion.rationale().is_empty()));
    let mut with_backups = targets(directory.path());
    with_backups.push(UserDataTarget::filesystem(
        UserDataTargetKind::MigrationBackupRoot,
        directory.path().join("another-exact-backup"),
    ));
    assert_eq!(
        validate(&directory, with_backups).unwrap().targets().len(),
        UserDataTargetKind::ALL.len() + 1
    );
}

#[cfg(unix)]
#[test]
fn inventory_rejects_missing_duplicate_escape_and_symlink_targets() {
    use std::os::unix::fs::symlink;
    let directory = tempfile::tempdir().unwrap();
    let mut missing = targets(directory.path());
    missing.pop();
    assert_eq!(validate(&directory, missing), Err(StorageError::CommandConflict));

    let mut duplicate = targets(directory.path());
    duplicate[1].location = duplicate[0].location.clone();
    assert_eq!(validate(&directory, duplicate), Err(StorageError::CommandConflict));

    let mut duplicate_kind = targets(directory.path());
    duplicate_kind.push(UserDataTarget::filesystem(
        UserDataTargetKind::CoreWal,
        directory.path().join("second-core-wal"),
    ));
    assert_eq!(validate(&directory, duplicate_kind), Err(StorageError::CommandConflict));

    let mut escape = targets(directory.path());
    escape[0].location = crate::UserDataTargetLocation::Filesystem(
        std::env::temp_dir().join("pod0-erasure-outside.sqlite")
    );
    assert_eq!(validate(&directory, escape), Err(StorageError::CommandConflict));

    let link = directory.path().join("linked");
    symlink(directory.path().join("real"), &link).unwrap();
    let mut linked = targets(directory.path());
    linked[0].location = crate::UserDataTargetLocation::Filesystem(link.join("core.sqlite"));
    assert_eq!(validate(&directory, linked), Err(StorageError::CommandConflict));
}

#[test]
fn covered_targets_require_the_exact_machine_checked_relationship() {
    let directory = tempfile::tempdir().unwrap();
    let mut wrong_cover = targets(directory.path());
    let transcript = wrong_cover
        .iter_mut()
        .find(|target| target.kind == UserDataTargetKind::TranscriptArtifactRoot)
        .unwrap();
    transcript.location = crate::UserDataTargetLocation::CoveredBy {
        kind: UserDataTargetKind::EpisodeSqlite,
    };
    assert_eq!(
        validate(&directory, wrong_cover),
        Err(StorageError::CommandConflict)
    );

    let mut synthetic_path = targets(directory.path());
    let chapter = synthetic_path
        .iter_mut()
        .find(|target| target.kind == UserDataTargetKind::ChapterArtifactRoot)
        .unwrap();
    chapter.location = crate::UserDataTargetLocation::Filesystem(
        directory.path().join("synthetic-chapter-path"),
    );
    assert_eq!(
        validate(&directory, synthetic_path),
        Err(StorageError::CommandConflict)
    );
}

#[test]
fn recovery_uses_persisted_versioned_backup_inventory_after_quarantine() {
    use crate::recovery_test_support::Fixture;
    use crate::{
        UserDataErasureFaultPoint, prepare_user_data_erasure, recover_user_data_erasure,
    };
    let fixture = Fixture::new();
    fixture.migrate_to_current(910).unwrap();
    let root = fixture._directory.path();
    let base = root.join("core.sqlite.schema-backup");
    let dynamic = root.join("core.sqlite.schema-backup-v39");
    std::fs::write(&base, b"base backup").unwrap();
    std::fs::write(&dynamic, b"versioned backup").unwrap();
    let mut initial_targets = restart_targets(root, fixture.store.clone(), base.clone());
    initial_targets.push(UserDataTarget::filesystem(
        UserDataTargetKind::MigrationBackupRoot,
        dynamic.clone(),
    ));
    let initial = validate(&fixture._directory, initial_targets).unwrap();
    let dynamic_index = initial.targets().iter().position(|target| {
        target.path() == Some(dynamic.as_path())
    }).unwrap() as u16;
    let prepared = prepare_user_data_erasure(
        initial, root, pod0_domain::CommandId::from_parts(0, 910),
        pod0_domain::CommandId::from_parts(0, 911),
        pod0_domain::CommandId::from_parts(0, 912), b"{}".to_vec(),
    ).unwrap();
    assert!(crate::user_data_erasure::execute_erasure_with_fault(prepared, |point| {
        (point != UserDataErasureFaultPoint::AfterTargetRename { index: dynamic_index })
            .then_some(()).ok_or(StorageError::CommandConflict)
    }).is_err());
    assert!(!dynamic.exists());
    let restart = validate(
        &fixture._directory,
        restart_targets(root, fixture.store.clone(), base),
    ).unwrap();
    let marker = root.read_dir().unwrap().map(|entry| entry.unwrap().path())
        .find(|path| path.extension().is_some_and(|value| value == "json")).unwrap();
    assert!(recover_user_data_erasure(&marker, root, &restart).is_ok());
}

fn validate(
    directory: &tempfile::TempDir,
    targets: Vec<UserDataTarget>,
) -> Result<ValidatedUserDataInventory, StorageError> {
    ValidatedUserDataInventory::validate(targets, &[directory.path().to_path_buf()])
}

fn targets(root: &std::path::Path) -> Vec<UserDataTarget> {
    UserDataTargetKind::ALL
        .into_iter()
        .enumerate()
        .map(|(index, kind)| {
            if let Some(covering) = kind.covering_kind() {
                UserDataTarget::covered_by(kind, covering)
            } else {
                kind.native_action_identifier().map_or_else(
                    || UserDataTarget::filesystem(kind, root.join(format!("target-{index}"))),
                    |identifier| UserDataTarget::native(kind, identifier),
                )
            }
        })
        .collect()
}

fn restart_targets(
    root: &std::path::Path,
    core: std::path::PathBuf,
    base_backup: std::path::PathBuf,
) -> Vec<UserDataTarget> {
    targets(root).into_iter().map(|mut target| {
        match target.kind {
            UserDataTargetKind::CoreSqlite => {
                target.location = crate::UserDataTargetLocation::Filesystem(core.clone());
            }
            UserDataTargetKind::MigrationBackupRoot => {
                target.location = crate::UserDataTargetLocation::Filesystem(base_backup.clone());
            }
            _ => {}
        }
        target
    }).collect()
}
