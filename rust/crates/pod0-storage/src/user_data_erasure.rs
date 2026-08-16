use std::path::{Path, PathBuf};

use pod0_domain::CommandId;

use crate::user_data_erasure_filesystem::{
    cleanup_target, ensure_fresh_store, ensure_quarantine_root, quarantine_target,
    validate_empty_quarantine, validate_store_id,
};
use crate::user_data_erasure_marker::{
    ErasureMarker, ErasurePhase, MarkerLocation, TargetState, hex, new_marker, read_marker,
    sync_parent, write_marker,
};
use crate::user_data_erasure_projection::{
    ensure_sanitized_application_state, sanitized_application_state,
};
use crate::user_data_erasure_recovery::validate_recovery_marker;
use crate::{StorageError, UserDataTargetKind, ValidatedUserDataInventory};

#[derive(Debug)]
pub struct UserDataErasureConfirmation {
    pub(crate) marker_path: PathBuf,
    pub(crate) quarantine_root: PathBuf,
    pub(crate) marker: ErasureMarker,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UserDataErasureFaultPoint {
    AfterIntentMarker,
    AfterTargetRename { index: u16 },
    AfterFreshStore,
    AfterTargetCleanup { index: u16 },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LeasedNativeErasureAction {
    pub action_id: CommandId,
    pub operation_id: CommandId,
    pub kind: UserDataTargetKind,
    pub identifier: String,
    pub attempt: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UserDataErasureProgress {
    AwaitingNativeActions(Vec<LeasedNativeErasureAction>),
    Complete(CommandId),
}

pub fn prepare_user_data_erasure(
    inventory: ValidatedUserDataInventory,
    recovery_root: &Path,
    expected_store_id: CommandId,
    fresh_store_id: CommandId,
    operation_id: CommandId,
    retained_settings_json: Vec<u8>,
) -> Result<UserDataErasureConfirmation, StorageError> {
    if expected_store_id == fresh_store_id || expected_store_id == operation_id {
        return Err(StorageError::CommandConflict);
    }
    let root = std::fs::canonicalize(recovery_root).map_err(|_| StorageError::CommandConflict)?;
    if !root.is_dir() {
        return Err(StorageError::CommandConflict);
    }
    let core = inventory
        .targets()
        .iter()
        .find(|target| target.kind == UserDataTargetKind::CoreSqlite)
        .ok_or(StorageError::CommandConflict)?;
    validate_store_id(
        core.path().ok_or(StorageError::CommandConflict)?,
        expected_store_id,
    )?;
    let suffix = hex(&operation_id.into_bytes());
    let marker_path = root.join(format!("pod0-erasure-{suffix}.json"));
    let quarantine_root = root.join(format!("pod0-erasure-{suffix}.quarantine"));
    if marker_path.exists() || quarantine_root.exists() {
        return Err(StorageError::CommandConflict);
    }
    let sanitized = sanitized_application_state(&retained_settings_json)?;
    Ok(UserDataErasureConfirmation {
        marker: new_marker(
            &inventory,
            &quarantine_root,
            operation_id,
            expected_store_id,
            fresh_store_id,
            sanitized,
        )?,
        marker_path,
        quarantine_root,
    })
}

pub fn confirm_user_data_erasure(
    confirmation: UserDataErasureConfirmation,
) -> Result<UserDataErasureProgress, StorageError> {
    execute_with_fault(confirmation, |_| Ok(()))
}

pub fn recover_user_data_erasure(
    marker_path: &Path,
    recovery_root: &Path,
    inventory: &ValidatedUserDataInventory,
) -> Result<UserDataErasureProgress, StorageError> {
    let root = std::fs::canonicalize(recovery_root).map_err(|_| StorageError::CommandConflict)?;
    let marker_file =
        std::fs::canonicalize(marker_path).map_err(|_| StorageError::CommandConflict)?;
    if marker_file.parent() != Some(root.as_path()) {
        return Err(StorageError::CommandConflict);
    }
    let marker = read_marker(&marker_file)?;
    validate_recovery_marker(&marker, &marker_file, &root, inventory)?;
    execute_with_fault(
        UserDataErasureConfirmation {
            quarantine_root: marker.quarantine_root.clone(),
            marker_path: marker_file,
            marker,
        },
        |_| Ok(()),
    )
}

pub fn observe_native_user_data_erasure(
    marker_path: &Path,
    recovery_root: &Path,
    inventory: &ValidatedUserDataInventory,
    action_id: CommandId,
    observed_attempt: u16,
    succeeded: bool,
) -> Result<UserDataErasureProgress, StorageError> {
    observe_native_with_fault(
        marker_path,
        recovery_root,
        inventory,
        action_id,
        observed_attempt,
        succeeded,
        |_| Ok(()),
    )
}

fn observe_native_with_fault(
    marker_path: &Path,
    recovery_root: &Path,
    inventory: &ValidatedUserDataInventory,
    action_id: CommandId,
    observed_attempt: u16,
    succeeded: bool,
    fault: impl FnMut(UserDataErasureFaultPoint) -> Result<(), StorageError>,
) -> Result<UserDataErasureProgress, StorageError> {
    let root = std::fs::canonicalize(recovery_root).map_err(|_| StorageError::CommandConflict)?;
    let marker_file =
        std::fs::canonicalize(marker_path).map_err(|_| StorageError::CommandConflict)?;
    let mut marker = read_marker(&marker_file)?;
    validate_recovery_marker(&marker, &marker_file, &root, inventory)?;
    let target = marker
        .targets
        .iter_mut()
        .find(|target| {
            matches!(&target.location, MarkerLocation::NativeAction { action_id: id, .. } if *id == action_id)
        })
        .ok_or(StorageError::CommandConflict)?;
    let MarkerLocation::NativeAction { attempt, .. } = &mut target.location else {
        return Err(StorageError::CommandConflict);
    };
    if *attempt != observed_attempt {
        return Err(StorageError::CommandConflict);
    }
    match (target.state, succeeded) {
        (TargetState::NativeAuthorized, true) => target.state = TargetState::NativeCompleted,
        (TargetState::NativeCompleted, true) => {}
        (TargetState::NativeAuthorized, false) => {
            *attempt = attempt
                .checked_add(1)
                .ok_or(StorageError::CommandConflict)?;
        }
        _ => return Err(StorageError::CommandConflict),
    }
    write_marker(&marker_file, &marker)?;
    execute_with_fault(
        UserDataErasureConfirmation {
            quarantine_root: marker.quarantine_root.clone(),
            marker_path: marker_file,
            marker,
        },
        fault,
    )
}

fn execute_with_fault(
    mut prepared: UserDataErasureConfirmation,
    mut fault: impl FnMut(UserDataErasureFaultPoint) -> Result<(), StorageError>,
) -> Result<UserDataErasureProgress, StorageError> {
    if !prepared.marker_path.exists() {
        write_marker(&prepared.marker_path, &prepared.marker)?;
        ensure_quarantine_root(&prepared)?;
        fault(UserDataErasureFaultPoint::AfterIntentMarker)?;
    } else {
        ensure_quarantine_root(&prepared)?;
    }
    if prepared.marker.phase == ErasurePhase::Quarantining {
        for index in 0..prepared.marker.targets.len() {
            quarantine_target(&mut prepared, index)?;
            fault(UserDataErasureFaultPoint::AfterTargetRename {
                index: index as u16,
            })?;
        }
        prepared.marker.phase = ErasurePhase::Recreating;
        write_marker(&prepared.marker_path, &prepared.marker)?;
    }
    let actions = pending_native_actions(&prepared.marker);
    if !actions.is_empty() {
        return Ok(UserDataErasureProgress::AwaitingNativeActions(actions));
    }
    if prepared.marker.phase == ErasurePhase::Recreating {
        ensure_fresh_store(&prepared)?;
        ensure_sanitized_application_state(&prepared)?;
        prepared.marker.phase = ErasurePhase::Cleaning;
        write_marker(&prepared.marker_path, &prepared.marker)?;
        fault(UserDataErasureFaultPoint::AfterFreshStore)?;
    }
    for index in 0..prepared.marker.targets.len() {
        cleanup_target(&mut prepared, index)?;
        fault(UserDataErasureFaultPoint::AfterTargetCleanup {
            index: index as u16,
        })?;
    }
    validate_empty_quarantine(&prepared)?;
    std::fs::remove_dir(&prepared.quarantine_root)
        .map_err(|error| StorageError::io("remove erasure quarantine", error))?;
    std::fs::remove_file(&prepared.marker_path)
        .map_err(|error| StorageError::io("remove erasure marker", error))?;
    sync_parent(&prepared.marker_path)?;
    Ok(UserDataErasureProgress::Complete(
        prepared.marker.fresh_store_id,
    ))
}

fn pending_native_actions(marker: &ErasureMarker) -> Vec<LeasedNativeErasureAction> {
    marker
        .targets
        .iter()
        .filter_map(|target| {
            let MarkerLocation::NativeAction {
                identifier,
                action_id,
                attempt,
            } = &target.location
            else {
                return None;
            };
            (target.state == TargetState::NativeAuthorized).then(|| LeasedNativeErasureAction {
                action_id: *action_id,
                operation_id: marker.operation_id,
                kind: target.kind,
                identifier: identifier.clone(),
                attempt: *attempt,
            })
        })
        .collect()
}

#[cfg(test)]
pub(crate) fn execute_erasure_with_fault(
    prepared: UserDataErasureConfirmation,
    fault: impl FnMut(UserDataErasureFaultPoint) -> Result<(), StorageError>,
) -> Result<UserDataErasureProgress, StorageError> {
    execute_with_fault(prepared, fault)
}

#[cfg(test)]
pub(crate) fn observe_native_erasure_with_fault(
    marker_path: &Path,
    recovery_root: &Path,
    inventory: &ValidatedUserDataInventory,
    action_id: CommandId,
    observed_attempt: u16,
    succeeded: bool,
    fault: impl FnMut(UserDataErasureFaultPoint) -> Result<(), StorageError>,
) -> Result<UserDataErasureProgress, StorageError> {
    observe_native_with_fault(
        marker_path,
        recovery_root,
        inventory,
        action_id,
        observed_attempt,
        succeeded,
        fault,
    )
}
