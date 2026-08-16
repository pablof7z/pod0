use std::path::{Path, PathBuf};

use crate::user_data_erasure_marker::{ErasureMarker, MarkerLocation, hex, native_action_id};
use crate::user_data_erasure_projection::validate_sanitized_application_state;
use crate::{StorageError, UserDataTargetKind, UserDataTargetLocation, ValidatedUserDataInventory};

pub fn pending_user_data_erasure_markers(
    recovery_root: &Path,
) -> Result<Vec<PathBuf>, StorageError> {
    let root = std::fs::canonicalize(recovery_root).map_err(|_| StorageError::CommandConflict)?;
    let mut markers = Vec::new();
    for entry in std::fs::read_dir(&root)
        .map_err(|error| StorageError::io("enumerate erasure recovery markers", error))?
    {
        let path = entry
            .map_err(|error| StorageError::io("read erasure recovery marker entry", error))?
            .path();
        let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        if !name.starts_with("pod0-erasure-") || !name.ends_with(".json") {
            continue;
        }
        let identity = &name[13..name.len() - 5];
        if identity.len() != 32
            || !identity
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(StorageError::CommandConflict);
        }
        markers.push(path);
    }
    markers.sort();
    Ok(markers)
}

pub(crate) fn validate_recovery_marker(
    marker: &ErasureMarker,
    marker_path: &Path,
    recovery_root: &Path,
    inventory: &ValidatedUserDataInventory,
) -> Result<(), StorageError> {
    let suffix = hex(&marker.operation_id.into_bytes());
    if marker_path != recovery_root.join(format!("pod0-erasure-{suffix}.json"))
        || marker.quarantine_root != recovery_root.join(format!("pod0-erasure-{suffix}.quarantine"))
        || marker.expected_store_id == marker.fresh_store_id
    {
        return Err(StorageError::CommandConflict);
    }
    validate_sanitized_application_state(marker.sanitized_application_state.as_bytes())?;
    for (index, stored) in marker.targets.iter().enumerate() {
        if !inventory.targets().iter().any(|expected| {
            stored.kind == expected.kind && location_matches(marker, index, stored, expected)
        }) && !valid_dynamic_backup(marker, index, stored, inventory)
        {
            return Err(StorageError::CommandConflict);
        }
    }
    for expected in inventory.targets() {
        if !marker.targets.iter().enumerate().any(|(index, stored)| {
            stored.kind == expected.kind && location_matches(marker, index, stored, expected)
        }) {
            return Err(StorageError::CommandConflict);
        }
    }
    Ok(())
}

fn valid_dynamic_backup(
    marker: &ErasureMarker,
    index: usize,
    stored: &crate::user_data_erasure_marker::MarkerTarget,
    inventory: &ValidatedUserDataInventory,
) -> bool {
    if stored.kind != UserDataTargetKind::MigrationBackupRoot {
        return false;
    }
    let MarkerLocation::Filesystem {
        source, quarantine, ..
    } = &stored.location
    else {
        return false;
    };
    let Some(name) = source.file_name().and_then(|value| value.to_str()) else {
        return false;
    };
    let namespace_matches = inventory.targets().iter().any(|target| {
        if target.kind != UserDataTargetKind::MigrationBackupRoot {
            return false;
        }
        let Some(base) = target
            .path()
            .and_then(|path| path.file_name())
            .and_then(|v| v.to_str())
        else {
            return false;
        };
        valid_versioned_schema_backup(name, base) || valid_clip_backup(name, base)
    });
    namespace_matches
        && inventory.validates_recovery_path(source)
        && quarantine
            == &marker
                .quarantine_root
                .join(format!("{index:02}-{}", stored.kind.wire_name()))
}

fn valid_versioned_schema_backup(name: &str, base: &str) -> bool {
    base.ends_with(".schema-backup")
        && name
            .strip_prefix(&format!("{base}-v"))
            .is_some_and(|suffix| {
                !suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_digit())
            })
}

fn valid_clip_backup(name: &str, base: &str) -> bool {
    base.ends_with(".clips-backup")
        && name
            .strip_prefix(&format!("{base}-"))
            .is_some_and(|suffix| {
                !suffix.is_empty()
                    && suffix
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
            })
}

fn location_matches(
    marker: &ErasureMarker,
    index: usize,
    stored: &crate::user_data_erasure_marker::MarkerTarget,
    expected: &crate::UserDataTarget,
) -> bool {
    match (&stored.location, &expected.location) {
        (
            MarkerLocation::Filesystem {
                source, quarantine, ..
            },
            UserDataTargetLocation::Filesystem(expected_source),
        ) => {
            source == expected_source
                && quarantine
                    == &marker
                        .quarantine_root
                        .join(format!("{index:02}-{}", stored.kind.wire_name()))
        }
        (
            MarkerLocation::NativeAction {
                identifier,
                action_id,
                attempt,
            },
            UserDataTargetLocation::NativeAction {
                identifier: expected_identifier,
            },
        ) => {
            identifier == expected_identifier
                && *action_id == native_action_id(marker.operation_id, stored.kind)
                && *attempt > 0
        }
        (
            MarkerLocation::CoveredBy { kind },
            UserDataTargetLocation::CoveredBy {
                kind: expected_kind,
            },
        ) => kind == expected_kind,
        _ => false,
    }
}
