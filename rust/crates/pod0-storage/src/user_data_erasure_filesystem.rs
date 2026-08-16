use std::path::Path;

use pod0_domain::CommandId;

use crate::user_data_erasure::UserDataErasureConfirmation;
use crate::user_data_erasure_evidence::verify_evidence;
use crate::user_data_erasure_marker::{MarkerLocation, TargetState, sync_parent, write_marker};
use crate::{CoreStoreMigrator, MigrationClock, StorageError, UserDataTargetKind};

pub(crate) fn quarantine_target(
    prepared: &mut UserDataErasureConfirmation,
    index: usize,
) -> Result<(), StorageError> {
    if let MarkerLocation::CoveredBy { kind } = prepared.marker.targets[index].location {
        let covering_state = prepared
            .marker
            .targets
            .iter()
            .find(|target| target.kind == kind)
            .map(|target| target.state)
            .ok_or(StorageError::CommandConflict)?;
        if !matches!(
            covering_state,
            TargetState::Absent | TargetState::Quarantined
        ) {
            return Err(StorageError::CommandConflict);
        }
        let target = &mut prepared.marker.targets[index];
        if target.state == TargetState::Pending {
            target.state = TargetState::Covered;
            return write_marker(&prepared.marker_path, &prepared.marker);
        }
        return matches!(target.state, TargetState::Covered | TargetState::Removed)
            .then_some(())
            .ok_or(StorageError::CommandConflict);
    }
    let target = &mut prepared.marker.targets[index];
    if target.state != TargetState::Pending {
        return validate_quarantined_state(target);
    }
    let MarkerLocation::Filesystem {
        source,
        quarantine,
        evidence,
    } = &target.location
    else {
        target.state = TargetState::NativeAuthorized;
        return write_marker(&prepared.marker_path, &prepared.marker);
    };
    target.state = match (source.exists(), quarantine.exists(), evidence.existed) {
        (true, false, true) => {
            verify_evidence(source, evidence)?;
            std::fs::rename(source, quarantine)
                .map_err(|error| StorageError::io("quarantine user data target", error))?;
            sync_parent(source)?;
            sync_parent(quarantine)?;
            TargetState::Quarantined
        }
        (false, true, true) => {
            verify_evidence(quarantine, evidence)?;
            TargetState::Quarantined
        }
        (false, false, false) => TargetState::Absent,
        _ => return Err(StorageError::CommandConflict),
    };
    write_marker(&prepared.marker_path, &prepared.marker)
}

pub(crate) fn cleanup_target(
    prepared: &mut UserDataErasureConfirmation,
    index: usize,
) -> Result<(), StorageError> {
    let target = &mut prepared.marker.targets[index];
    if matches!(
        target.state,
        TargetState::Pending | TargetState::NativeAuthorized
    ) {
        return Err(StorageError::CommandConflict);
    }
    let MarkerLocation::Filesystem {
        quarantine,
        evidence,
        ..
    } = &target.location
    else {
        target.state = TargetState::Removed;
        return write_marker(&prepared.marker_path, &prepared.marker);
    };
    if target.state == TargetState::Quarantined && quarantine.exists() {
        verify_evidence(quarantine, evidence)?;
        let metadata = std::fs::symlink_metadata(quarantine)
            .map_err(|error| StorageError::io("read quarantined user data", error))?;
        if metadata.is_dir() {
            std::fs::remove_dir_all(quarantine)
        } else {
            std::fs::remove_file(quarantine)
        }
        .map_err(|error| StorageError::io("remove quarantined user data", error))?;
        sync_parent(quarantine)?;
    }
    if quarantine.exists() {
        return Err(StorageError::CommandConflict);
    }
    target.state = TargetState::Removed;
    write_marker(&prepared.marker_path, &prepared.marker)
}

pub(crate) fn ensure_fresh_store(
    prepared: &UserDataErasureConfirmation,
) -> Result<(), StorageError> {
    let core = prepared
        .marker
        .targets
        .iter()
        .find(|target| target.kind == UserDataTargetKind::CoreSqlite)
        .ok_or(StorageError::CommandConflict)?;
    let MarkerLocation::Filesystem { source, .. } = &core.location else {
        return Err(StorageError::CommandConflict);
    };
    if source.exists() {
        return validate_store_id(source, prepared.marker.fresh_store_id);
    }
    CoreStoreMigrator::new(ErasureClock).migrate(
        source,
        crate::CURRENT_SCHEMA_VERSION,
        &prepared.quarantine_root.join("fresh-store.backup"),
        prepared.marker.fresh_store_id,
    )?;
    validate_store_id(source, prepared.marker.fresh_store_id)
}

pub(crate) fn ensure_quarantine_root(
    prepared: &UserDataErasureConfirmation,
) -> Result<(), StorageError> {
    if prepared.quarantine_root.exists() {
        let metadata = std::fs::symlink_metadata(&prepared.quarantine_root)
            .map_err(|error| StorageError::io("inspect erasure quarantine", error))?;
        return (metadata.is_dir() && !metadata.file_type().is_symlink())
            .then_some(())
            .ok_or(StorageError::CommandConflict);
    }
    std::fs::create_dir(&prepared.quarantine_root)
        .map_err(|error| StorageError::io("create erasure quarantine", error))?;
    sync_parent(&prepared.quarantine_root)
}

pub(crate) fn validate_empty_quarantine(
    prepared: &UserDataErasureConfirmation,
) -> Result<(), StorageError> {
    let empty = std::fs::read_dir(&prepared.quarantine_root)
        .map_err(|error| StorageError::io("inspect erasure quarantine cleanup", error))?
        .next()
        .is_none();
    empty.then_some(()).ok_or(StorageError::CommandConflict)
}

pub(crate) fn validate_store_id(path: &Path, expected: CommandId) -> Result<(), StorageError> {
    let connection =
        rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|error| StorageError::sqlite("open erasure identity", error))?;
    let bytes: Vec<u8> = connection
        .query_row(
            "SELECT store_id FROM pod0_store_metadata WHERE singleton=1",
            [],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("read erasure identity", error))?;
    (bytes == expected.into_bytes())
        .then_some(())
        .ok_or(StorageError::CommandConflict)
}

fn validate_quarantined_state(
    target: &crate::user_data_erasure_marker::MarkerTarget,
) -> Result<(), StorageError> {
    match target.state {
        TargetState::NativeAuthorized | TargetState::NativeCompleted
            if matches!(target.location, MarkerLocation::NativeAction { .. }) =>
        {
            Ok(())
        }
        TargetState::Covered | TargetState::Removed
            if matches!(target.location, MarkerLocation::CoveredBy { .. }) =>
        {
            Ok(())
        }
        TargetState::Absent if filesystem_state(target, false, false)? => Ok(()),
        TargetState::Quarantined if filesystem_state(target, true, true)? => Ok(()),
        TargetState::Removed if filesystem_state(target, false, true)? => Ok(()),
        _ => Err(StorageError::CommandConflict),
    }
}

fn filesystem_state(
    target: &crate::user_data_erasure_marker::MarkerTarget,
    quarantine_exists: bool,
    originally_existed: bool,
) -> Result<bool, StorageError> {
    let MarkerLocation::Filesystem {
        quarantine,
        evidence,
        ..
    } = &target.location
    else {
        return Ok(false);
    };
    if quarantine.exists() != quarantine_exists || evidence.existed != originally_existed {
        return Ok(false);
    }
    if quarantine_exists {
        verify_evidence(quarantine, evidence)?;
    }
    Ok(true)
}

struct ErasureClock;
impl MigrationClock for ErasureClock {
    fn now_milliseconds(&self) -> i64 {
        1
    }
}
