use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};

use crate::StorageError;

#[path = "user_data_erasure_inventory_kind.rs"]
mod kind;
pub use kind::UserDataTargetKind;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UserDataTarget {
    pub kind: UserDataTargetKind,
    pub location: UserDataTargetLocation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UserDataTargetLocation {
    Filesystem(PathBuf),
    NativeAction { identifier: String },
    CoveredBy { kind: UserDataTargetKind },
}

impl UserDataTarget {
    pub fn filesystem(kind: UserDataTargetKind, path: PathBuf) -> Self {
        Self {
            kind,
            location: UserDataTargetLocation::Filesystem(path),
        }
    }

    pub fn native(kind: UserDataTargetKind, identifier: impl Into<String>) -> Self {
        Self {
            kind,
            location: UserDataTargetLocation::NativeAction {
                identifier: identifier.into(),
            },
        }
    }

    pub fn covered_by(kind: UserDataTargetKind, covering_kind: UserDataTargetKind) -> Self {
        Self {
            kind,
            location: UserDataTargetLocation::CoveredBy {
                kind: covering_kind,
            },
        }
    }

    pub fn path(&self) -> Option<&Path> {
        match &self.location {
            UserDataTargetLocation::Filesystem(path) => Some(path),
            UserDataTargetLocation::NativeAction { .. }
            | UserDataTargetLocation::CoveredBy { .. } => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedUserDataInventory {
    targets: Vec<UserDataTarget>,
    roots: Vec<PathBuf>,
}

impl ValidatedUserDataInventory {
    pub fn validate(
        targets: Vec<UserDataTarget>,
        allowed_roots: &[PathBuf],
    ) -> Result<Self, StorageError> {
        let roots = validate_roots(allowed_roots)?;
        let mut by_kind = BTreeMap::new();
        let mut paths = BTreeSet::new();
        for target in &targets {
            let count = by_kind.entry(target.kind).or_insert(0usize);
            if *count > 0 && !target.kind.allows_multiple_targets() {
                return Err(StorageError::CommandConflict);
            }
            *count += 1;
            match &target.location {
                UserDataTargetLocation::Filesystem(path) => {
                    if target.kind.native_action_identifier().is_some()
                        || target.kind.covering_kind().is_some()
                    {
                        return Err(StorageError::CommandConflict);
                    }
                    validate_target_path(path, &roots)?;
                    if !paths.insert(normalize(path)?) {
                        return Err(StorageError::CommandConflict);
                    }
                }
                UserDataTargetLocation::NativeAction { identifier } => {
                    if target.kind.native_action_identifier() != Some(identifier.as_str())
                        || target.kind.covering_kind().is_some()
                    {
                        return Err(StorageError::CommandConflict);
                    }
                }
                UserDataTargetLocation::CoveredBy { kind } => {
                    if target.kind.covering_kind() != Some(*kind) {
                        return Err(StorageError::CommandConflict);
                    }
                }
            }
        }
        if UserDataTargetKind::ALL
            .iter()
            .any(|kind| !by_kind.contains_key(kind))
        {
            return Err(StorageError::CommandConflict);
        }
        for target in &targets {
            let UserDataTargetLocation::CoveredBy { kind } = target.location else {
                continue;
            };
            let covering = targets
                .iter()
                .find(|candidate| candidate.kind == kind)
                .ok_or(StorageError::CommandConflict)?;
            if !matches!(covering.location, UserDataTargetLocation::Filesystem(_)) {
                return Err(StorageError::CommandConflict);
            }
        }
        let mut targets = targets;
        targets.sort_by(|left, right| {
            left.kind
                .cmp(&right.kind)
                .then_with(|| location_key(&left.location).cmp(&location_key(&right.location)))
        });
        Ok(Self { targets, roots })
    }

    pub fn targets(&self) -> &[UserDataTarget] {
        &self.targets
    }

    pub(crate) fn validates_recovery_path(&self, path: &Path) -> bool {
        validate_target_path(path, &self.roots).is_ok()
    }
}

fn location_key(location: &UserDataTargetLocation) -> String {
    match location {
        UserDataTargetLocation::Filesystem(path) => path.to_string_lossy().into_owned(),
        UserDataTargetLocation::NativeAction { identifier } => identifier.clone(),
        UserDataTargetLocation::CoveredBy { kind } => format!("covered_by:{}", kind.wire_name()),
    }
}

fn validate_roots(roots: &[PathBuf]) -> Result<Vec<PathBuf>, StorageError> {
    if roots.is_empty() {
        return Err(StorageError::CommandConflict);
    }
    let mut unique = BTreeSet::new();
    for root in roots {
        let canonical = std::fs::canonicalize(root).map_err(|_| StorageError::CommandConflict)?;
        if !canonical.is_dir() || !unique.insert(canonical) {
            return Err(StorageError::CommandConflict);
        }
    }
    Ok(unique.into_iter().collect())
}

fn validate_target_path(path: &Path, roots: &[PathBuf]) -> Result<(), StorageError> {
    let normalized = normalize(path)?;
    let resolved = resolve_existing_ancestor(&normalized)?;
    roots
        .iter()
        .any(|root| resolved.starts_with(root))
        .then_some(())
        .ok_or(StorageError::CommandConflict)
}

fn resolve_existing_ancestor(path: &Path) -> Result<PathBuf, StorageError> {
    let mut ancestor = path;
    let mut suffix = Vec::new();
    while !ancestor.exists() {
        if let Ok(metadata) = std::fs::symlink_metadata(ancestor)
            && metadata.file_type().is_symlink()
        {
            return Err(StorageError::CommandConflict);
        }
        suffix.push(
            ancestor
                .file_name()
                .ok_or(StorageError::CommandConflict)?
                .to_owned(),
        );
        ancestor = ancestor.parent().ok_or(StorageError::CommandConflict)?;
    }
    let metadata =
        std::fs::symlink_metadata(ancestor).map_err(|_| StorageError::CommandConflict)?;
    if metadata.file_type().is_symlink() {
        return Err(StorageError::CommandConflict);
    }
    let mut resolved =
        std::fs::canonicalize(ancestor).map_err(|_| StorageError::CommandConflict)?;
    for part in suffix.into_iter().rev() {
        resolved.push(part);
    }
    Ok(resolved)
}

fn normalize(path: &Path) -> Result<PathBuf, StorageError> {
    if !path.is_absolute() {
        return Err(StorageError::CommandConflict);
    }
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(_) | Component::RootDir | Component::Normal(_) => {
                normalized.push(component.as_os_str());
            }
            Component::CurDir => {}
            Component::ParentDir => return Err(StorageError::CommandConflict),
        }
    }
    Ok(normalized)
}
