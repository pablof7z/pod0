use std::io::Write as _;
use std::path::{Path, PathBuf};

use pod0_domain::CommandId;

use crate::user_data_erasure_evidence::{TargetEvidence, target_evidence};
use crate::{StorageError, UserDataTargetKind, UserDataTargetLocation, ValidatedUserDataInventory};

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ErasurePhase {
    Quarantining,
    Recreating,
    Cleaning,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TargetState {
    Pending,
    Covered,
    NativeAuthorized,
    NativeCompleted,
    Absent,
    Quarantined,
    Removed,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct MarkerTarget {
    pub kind: UserDataTargetKind,
    pub location: MarkerLocation,
    pub state: TargetState,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum MarkerLocation {
    Filesystem {
        source: PathBuf,
        quarantine: PathBuf,
        evidence: TargetEvidence,
    },
    NativeAction {
        identifier: String,
        action_id: CommandId,
        attempt: u16,
    },
    CoveredBy {
        kind: UserDataTargetKind,
    },
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct ErasureMarker {
    pub version: u8,
    pub operation_id: CommandId,
    pub expected_store_id: CommandId,
    pub fresh_store_id: CommandId,
    pub sanitized_application_state: String,
    pub quarantine_root: PathBuf,
    pub phase: ErasurePhase,
    pub targets: Vec<MarkerTarget>,
}

pub(crate) fn new_marker(
    inventory: &ValidatedUserDataInventory,
    quarantine_root: &Path,
    operation_id: CommandId,
    expected_store_id: CommandId,
    fresh_store_id: CommandId,
    sanitized_application_state: Vec<u8>,
) -> Result<ErasureMarker, StorageError> {
    Ok(ErasureMarker {
        version: 3,
        operation_id,
        expected_store_id,
        fresh_store_id,
        sanitized_application_state: String::from_utf8(sanitized_application_state)
            .map_err(|_| StorageError::CommandConflict)?,
        quarantine_root: quarantine_root.to_path_buf(),
        phase: ErasurePhase::Quarantining,
        targets: inventory
            .targets()
            .iter()
            .enumerate()
            .map(|(index, target)| {
                let location = match &target.location {
                    UserDataTargetLocation::Filesystem(source) => MarkerLocation::Filesystem {
                        source: source.clone(),
                        quarantine: quarantine_root
                            .join(format!("{index:02}-{}", target.kind.wire_name())),
                        evidence: target_evidence(source)?,
                    },
                    UserDataTargetLocation::NativeAction { identifier } => {
                        MarkerLocation::NativeAction {
                            identifier: identifier.clone(),
                            action_id: native_action_id(operation_id, target.kind),
                            attempt: 1,
                        }
                    }
                    UserDataTargetLocation::CoveredBy { kind } => {
                        MarkerLocation::CoveredBy { kind: *kind }
                    }
                };
                Ok(MarkerTarget {
                    kind: target.kind,
                    location,
                    state: TargetState::Pending,
                })
            })
            .collect::<Result<Vec<_>, StorageError>>()?,
    })
}

pub(crate) fn native_action_id(operation_id: CommandId, kind: UserDataTargetKind) -> CommandId {
    use sha2::{Digest as _, Sha256};
    let mut hash = Sha256::new();
    hash.update(b"pod0-user-data-erasure-native-action-v1\0");
    hash.update(operation_id.into_bytes());
    hash.update(kind.wire_name().as_bytes());
    CommandId::from_bytes(hash.finalize()[..16].try_into().expect("digest prefix"))
}

pub(crate) fn read_marker(path: &Path) -> Result<ErasureMarker, StorageError> {
    let bytes =
        std::fs::read(path).map_err(|error| StorageError::io("read erasure marker", error))?;
    let marker: ErasureMarker =
        serde_json::from_slice(&bytes).map_err(|_| StorageError::CommandConflict)?;
    (marker.version == 3)
        .then_some(marker)
        .ok_or(StorageError::CommandConflict)
}

pub(crate) fn write_marker(path: &Path, marker: &ErasureMarker) -> Result<(), StorageError> {
    let temporary = path.with_extension("json.next");
    if temporary.exists() {
        std::fs::remove_file(&temporary)
            .map_err(|error| StorageError::io("clear stale erasure marker write", error))?;
    }
    let bytes = serde_json::to_vec(marker).map_err(|_| StorageError::CommandConflict)?;
    let mut file = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temporary)
        .map_err(|error| StorageError::io("create erasure marker", error))?;
    file.write_all(&bytes)
        .and_then(|_| file.sync_all())
        .map_err(|error| StorageError::io("sync erasure marker", error))?;
    std::fs::rename(&temporary, path)
        .map_err(|error| StorageError::io("publish erasure marker", error))?;
    sync_parent(path)
}

pub(crate) fn sync_parent(path: &Path) -> Result<(), StorageError> {
    let parent = path.parent().ok_or(StorageError::CommandConflict)?;
    std::fs::File::open(parent)
        .map_err(|error| StorageError::io("open erasure directory for sync", error))?
        .sync_all()
        .map_err(|error| StorageError::io("sync erasure directory", error))
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
