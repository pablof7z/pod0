use pod0_domain::CommandId;

use crate::recovery_test_support::Fixture;
use crate::user_data_erasure_marker::{MarkerLocation, read_marker};
use crate::{
    StorageError, UserDataErasureFaultPoint, UserDataTarget, UserDataTargetKind,
    UserDataErasureProgress, ValidatedUserDataInventory, confirm_user_data_erasure,
    observe_native_user_data_erasure, prepare_user_data_erasure, recover_user_data_erasure,
};

#[test]
fn identity_failure_is_byte_exact_noop_and_success_recreates_a_fresh_identity() {
    let fixture = Fixture::new();
    fixture.migrate_to_current(219).unwrap();
    let before = std::fs::read(&fixture.store).unwrap();
    let inventory = inventory(fixture._directory.path(), fixture.store.clone());
    assert!(matches!(
        prepare_user_data_erasure(
            inventory.clone(), fixture._directory.path(), id(999), id(220), id(221), b"{}".to_vec(),
        ),
        Err(StorageError::CommandConflict)
    ));
    assert_eq!(std::fs::read(&fixture.store).unwrap(), before);

    let prepared = prepare_user_data_erasure(
        inventory.clone(), fixture._directory.path(), id(219), id(220), id(221), b"{}".to_vec(),
    ).unwrap();
    let progress = confirm_user_data_erasure(prepared).unwrap();
    assert_eq!(finish_native(progress, fixture._directory.path(), &inventory), id(220));
    let connection = rusqlite::Connection::open(&fixture.store).unwrap();
    let stored: Vec<u8> = connection.query_row(
        "SELECT store_id FROM pod0_store_metadata WHERE singleton=1", [], |row| row.get(0),
    ).unwrap();
    assert_eq!(stored, id(220).into_bytes());
}

#[test]
fn every_fault_seam_reopens_from_marker_and_converges_forward() {
    let baseline = Fixture::new();
    let old_id = id(300);
    baseline.migrator.migrate(
        &baseline.store, crate::CURRENT_SCHEMA_VERSION, &baseline.backup, old_id,
    ).unwrap();
    let core_bytes = std::fs::read(&baseline.store).unwrap();
    let target_count = UserDataTargetKind::ALL.len();
    let mut seams = vec![UserDataErasureFaultPoint::AfterIntentMarker];
    seams.extend((0..target_count as u16).map(|index| UserDataErasureFaultPoint::AfterTargetRename { index }));
    seams.push(UserDataErasureFaultPoint::AfterFreshStore);
    seams.extend((0..target_count as u16).map(|index| UserDataErasureFaultPoint::AfterTargetCleanup { index }));
    for (offset, seam) in seams.into_iter().enumerate() {
        run_fault_and_recover(seam, offset as u64, old_id, &core_bytes);
    }
}

#[test]
fn recovery_rejects_quarantine_tamper_without_deleting_evidence() {
    let fixture = Fixture::new();
    fixture.migrate_to_current(700).unwrap();
    let target = fixture._directory.path().join("erasure-target-1");
    std::fs::write(&target, b"private bytes").unwrap();
    let inventory = inventory(fixture._directory.path(), fixture.store.clone());
    let prepared = prepare_user_data_erasure(
        inventory.clone(), fixture._directory.path(), id(700), id(701), id(702), b"{}".to_vec(),
    ).unwrap();
    assert!(crate::user_data_erasure::execute_erasure_with_fault(prepared, |point| {
        (point != UserDataErasureFaultPoint::AfterTargetRename { index: 1 })
            .then_some(())
            .ok_or(StorageError::CommandConflict)
    }).is_err());
    let marker = erasure_marker(fixture._directory.path());
    let persisted = read_marker(&marker).unwrap();
    let MarkerLocation::Filesystem { quarantine: quarantined, .. } = &persisted.targets[1].location
    else { panic!("filesystem target") };
    let quarantined = quarantined.clone();
    std::fs::write(&quarantined, b"tampered bytes").unwrap();
    assert!(matches!(
        recover_user_data_erasure(&marker, fixture._directory.path(), &inventory),
        Err(StorageError::CommandConflict)
    ));
    assert!(marker.exists());
    assert_eq!(std::fs::read(quarantined).unwrap(), b"tampered bytes");
}

#[test]
fn native_action_failure_releases_a_new_attempt_and_stale_observation_fails_closed() {
    let fixture = Fixture::new();
    fixture.migrate_to_current(800).unwrap();
    let inventory = inventory(fixture._directory.path(), fixture.store.clone());
    let prepared = prepare_user_data_erasure(
        inventory.clone(),
        fixture._directory.path(),
        id(800),
        id(801),
        id(802),
        b"{}".to_vec(),
    )
    .unwrap();
    let UserDataErasureProgress::AwaitingNativeActions(actions) =
        confirm_user_data_erasure(prepared).unwrap()
    else {
        panic!("native erasure action expected")
    };
    let first = actions.first().unwrap();
    let marker = erasure_marker(fixture._directory.path());
    let retried = observe_native_user_data_erasure(
        &marker,
        fixture._directory.path(),
        &inventory,
        first.action_id,
        first.attempt,
        false,
    )
    .unwrap();
    let UserDataErasureProgress::AwaitingNativeActions(actions) = retried else {
        panic!("retry expected")
    };
    let retried = actions
        .iter()
        .find(|action| action.action_id == first.action_id)
        .unwrap();
    assert_eq!(retried.attempt, first.attempt + 1);
    assert!(matches!(
        observe_native_user_data_erasure(
            &marker,
            fixture._directory.path(),
            &inventory,
            first.action_id,
            first.attempt,
            true,
        ),
        Err(StorageError::CommandConflict)
    ));
}

fn run_fault_and_recover(
    seam: UserDataErasureFaultPoint,
    offset: u64,
    old_id: CommandId,
    core_bytes: &[u8],
) {
    let directory = tempfile::tempdir().unwrap();
    let core = directory.path().join("core.sqlite");
    std::fs::write(&core, core_bytes).unwrap();
    let fresh_id = id(301 + offset * 10);
    let targets = populated_targets(directory.path(), core.clone());
    let old_bytes = std::fs::read(&core).unwrap();
    let inventory = ValidatedUserDataInventory::validate(
        targets.clone(), &[directory.path().to_path_buf()],
    ).unwrap();
    let prepared = prepare_user_data_erasure(
        inventory.clone(), directory.path(), old_id, fresh_id, id(302 + offset * 10), b"{}".to_vec(),
    ).unwrap();
    let mut result = crate::user_data_erasure::execute_erasure_with_fault(prepared, |point| {
        (point != seam).then_some(()).ok_or(StorageError::CommandConflict)
    });
    while let Ok(UserDataErasureProgress::AwaitingNativeActions(actions)) = &result {
        let action = actions.first().unwrap();
        let marker = erasure_marker(directory.path());
        result = crate::user_data_erasure::observe_native_erasure_with_fault(
            &marker,
            directory.path(),
            &inventory,
            action.action_id,
            action.attempt,
            true,
            |point| {
                (point != seam)
                    .then_some(())
                    .ok_or(StorageError::CommandConflict)
            },
        );
    }
    assert!(matches!(result, Err(StorageError::CommandConflict)));
    let marker = erasure_marker(directory.path());
    if offset == 0 {
        let persisted = read_marker(&marker).unwrap();
        for (index, target) in persisted.targets.iter().enumerate() {
            if let MarkerLocation::Filesystem { quarantine, evidence, .. } = &target.location {
                assert!(evidence.existed);
                assert!(evidence.byte_count > 0);
                assert_eq!(evidence.sha256.as_ref().unwrap().len(), 64);
                assert_eq!(quarantine.file_name().unwrap().to_string_lossy(),
                    format!("{index:02}-{}", target.kind.wire_name()));
            }
        }
    }
    let progress = recover_user_data_erasure(&marker, directory.path(), &inventory).unwrap();
    assert_eq!(finish_native(progress, directory.path(), &inventory), fresh_id);
    assert!(!marker.exists());
    assert!(erasure_entries(directory.path()).is_empty());
    assert_ne!(std::fs::read(&core).unwrap(), old_bytes);
    assert_eq!(stored_id(&core), fresh_id.into_bytes());
    for target in targets {
        if target.kind == UserDataTargetKind::ApplicationStateProjection {
            let bytes = std::fs::read(target.path().unwrap()).unwrap();
            crate::user_data_erasure_projection::validate_sanitized_application_state(&bytes)
                .unwrap();
        } else if target.kind != UserDataTargetKind::CoreSqlite {
            if let Some(path) = target.path() {
                assert!(!path.exists(), "old target survived: {:?}", target.kind);
            }
        }
    }
}

fn inventory(root: &std::path::Path, core: std::path::PathBuf) -> ValidatedUserDataInventory {
    let targets = UserDataTargetKind::ALL.into_iter().enumerate().map(|(index, kind)| {
        let path = if kind == UserDataTargetKind::CoreSqlite {
            core.clone()
        } else { root.join(format!("erasure-target-{index}")) };
        target(kind, path)
    }).collect();
    ValidatedUserDataInventory::validate(targets, &[root.to_path_buf()]).unwrap()
}

fn populated_targets(root: &std::path::Path, core: std::path::PathBuf) -> Vec<UserDataTarget> {
    UserDataTargetKind::ALL.into_iter().enumerate().map(|(index, kind)| {
        let path = if kind == UserDataTargetKind::CoreSqlite {
            core.clone()
        } else {
            root.join(format!("erasure-target-{index}"))
        };
        if kind != UserDataTargetKind::CoreSqlite && kind.native_action_identifier().is_none() {
            if index % 2 == 0 {
                std::fs::create_dir(&path).unwrap();
                std::fs::write(path.join("private.bin"), [index as u8; 7]).unwrap();
            } else {
                std::fs::write(&path, [index as u8; 7]).unwrap();
            }
        }
        target(kind, path)
    }).collect()
}

fn erasure_marker(root: &std::path::Path) -> std::path::PathBuf {
    erasure_entries(root).into_iter().find(|path| path.extension().is_some_and(|value| value == "json"))
        .expect("erasure marker")
}

fn erasure_entries(root: &std::path::Path) -> Vec<std::path::PathBuf> {
    root.read_dir().unwrap().filter_map(|entry| {
        let path = entry.unwrap().path();
        path.file_name().unwrap().to_string_lossy().starts_with("pod0-erasure-").then_some(path)
    }).collect()
}

fn stored_id(path: &std::path::Path) -> Vec<u8> {
    rusqlite::Connection::open(path).unwrap().query_row(
        "SELECT store_id FROM pod0_store_metadata WHERE singleton=1", [], |row| row.get(0),
    ).unwrap()
}

fn target(kind: UserDataTargetKind, path: std::path::PathBuf) -> UserDataTarget {
    if let Some(covering) = kind.covering_kind() {
        UserDataTarget::covered_by(kind, covering)
    } else {
        kind.native_action_identifier().map_or_else(
            || UserDataTarget::filesystem(kind, path),
            |identifier| UserDataTarget::native(kind, identifier),
        )
    }
}

fn finish_native(
    mut progress: UserDataErasureProgress,
    root: &std::path::Path,
    inventory: &ValidatedUserDataInventory,
) -> CommandId {
    loop {
        match progress {
            UserDataErasureProgress::Complete(id) => return id,
            UserDataErasureProgress::AwaitingNativeActions(actions) => {
                let marker = erasure_marker(root);
                let action = actions.into_iter().next().expect("pending native action");
                progress = observe_native_user_data_erasure(
                    &marker,
                    root,
                    inventory,
                    action.action_id,
                    action.attempt,
                    true,
                ).unwrap();
            }
        }
    }
}

fn id(value: u64) -> CommandId { CommandId::from_parts(0, value) }
