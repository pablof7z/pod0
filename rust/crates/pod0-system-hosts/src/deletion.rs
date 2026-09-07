#[cfg(any(target_os = "linux", target_os = "macos"))]
use std::{fs, path::Component};
use std::{
    path::{Path, PathBuf},
    time::SystemTime,
};

#[cfg(any(target_os = "linux", target_os = "macos"))]
use cap_std::{ambient_authority, fs::Dir};

use crate::{Capability, HostResult, SystemHostError};
#[cfg(any(target_os = "linux", target_os = "macos"))]
use crate::{Operation, PermissionSource};

const CAPABILITY: Capability = Capability::ExactFileDeletion;

#[cfg(any(target_os = "linux", target_os = "macos"))]
struct AllowedRoot {
    path: PathBuf,
    directory: Dir,
}

pub struct ExactFileDeleter {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    roots: Vec<AllowedRoot>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileDeletionEvidence {
    pub requested_path: PathBuf,
    pub allowed_root: PathBuf,
    pub deleted_at: SystemTime,
}

impl ExactFileDeleter {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    pub fn new(allowed_roots: impl IntoIterator<Item = PathBuf>) -> HostResult<Self> {
        let mut roots = allowed_roots
            .into_iter()
            .map(open_canonical_root)
            .collect::<HostResult<Vec<_>>>()?;
        if roots.is_empty() {
            return Err(SystemHostError::invalid(
                CAPABILITY,
                Operation::ConfigureDeletion,
                "at least one allowed root is required",
            ));
        }
        roots.sort_by(|left, right| {
            right
                .path
                .components()
                .count()
                .cmp(&left.path.components().count())
                .then_with(|| left.path.cmp(&right.path))
        });
        roots.dedup_by(|left, right| left.path == right.path);
        Ok(Self { roots })
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    pub fn new(_allowed_roots: impl IntoIterator<Item = PathBuf>) -> HostResult<Self> {
        Err(SystemHostError::unsupported(CAPABILITY))
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    pub fn allowed_roots(&self) -> impl ExactSizeIterator<Item = &Path> {
        self.roots.iter().map(|root| root.path.as_path())
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    pub fn allowed_roots(&self) -> impl ExactSizeIterator<Item = &Path> {
        std::iter::empty()
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    pub fn delete_exact(
        &self,
        requested_path: impl AsRef<Path>,
    ) -> HostResult<FileDeletionEvidence> {
        let requested_path = requested_path.as_ref();
        if !requested_path.is_absolute() {
            return Err(SystemHostError::invalid(
                CAPABILITY,
                Operation::DeleteFile,
                "file path must be absolute",
            ));
        }

        let (root, relative) = self.resolve_root(requested_path)?;
        let metadata = root
            .directory
            .symlink_metadata(relative)
            .map_err(|error| SystemHostError::from_io(CAPABILITY, Operation::DeleteFile, error))?;
        let file_type = metadata.file_type();
        if !(file_type.is_file() || file_type.is_symlink()) {
            return Err(SystemHostError::invalid(
                CAPABILITY,
                Operation::DeleteFile,
                "target must be a regular file or symbolic link",
            ));
        }

        root.directory
            .remove_file(relative)
            .map_err(|error| SystemHostError::from_io(CAPABILITY, Operation::DeleteFile, error))?;
        Ok(FileDeletionEvidence {
            requested_path: requested_path.to_path_buf(),
            allowed_root: root.path.clone(),
            deleted_at: SystemTime::now(),
        })
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    pub fn delete_exact(
        &self,
        _requested_path: impl AsRef<Path>,
    ) -> HostResult<FileDeletionEvidence> {
        Err(SystemHostError::unsupported(CAPABILITY))
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn resolve_root<'a>(
        &'a self,
        requested_path: &'a Path,
    ) -> HostResult<(&'a AllowedRoot, &'a Path)> {
        self.roots
            .iter()
            .find_map(|root| {
                let relative = requested_path.strip_prefix(&root.path).ok()?;
                if safe_relative_file_path(relative) {
                    Some((root, relative))
                } else {
                    None
                }
            })
            .ok_or_else(|| {
                SystemHostError::permission(
                    CAPABILITY,
                    Operation::DeleteFile,
                    PermissionSource::Policy,
                )
            })
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn open_canonical_root(path: PathBuf) -> HostResult<AllowedRoot> {
    if !path.is_absolute() {
        return Err(SystemHostError::invalid(
            CAPABILITY,
            Operation::ConfigureDeletion,
            "allowed root must be absolute",
        ));
    }
    let canonical = fs::canonicalize(&path).map_err(|error| {
        SystemHostError::from_io(CAPABILITY, Operation::ConfigureDeletion, error)
    })?;
    if canonical != path {
        return Err(SystemHostError::invalid(
            CAPABILITY,
            Operation::ConfigureDeletion,
            "allowed root must already be canonical",
        ));
    }
    let validated_metadata = fs::symlink_metadata(&path).map_err(|error| {
        SystemHostError::from_io(CAPABILITY, Operation::ConfigureDeletion, error)
    })?;
    if !validated_metadata.is_dir() {
        return Err(SystemHostError::invalid(
            CAPABILITY,
            Operation::ConfigureDeletion,
            "allowed root must be a directory",
        ));
    }
    let opened = open_root_nofollow(&path).map_err(|error| {
        SystemHostError::from_io(CAPABILITY, Operation::ConfigureDeletion, error)
    })?;
    let opened_metadata = opened.metadata().map_err(|error| {
        SystemHostError::from_io(CAPABILITY, Operation::ConfigureDeletion, error)
    })?;
    if !same_file(&validated_metadata, &opened_metadata) {
        return Err(SystemHostError::invalid(
            CAPABILITY,
            Operation::ConfigureDeletion,
            "allowed root changed while it was being opened",
        ));
    }
    let directory = Dir::from_std_file(opened);
    Ok(AllowedRoot { path, directory })
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn open_root_nofollow(path: &Path) -> std::io::Result<fs::File> {
    let (anchor_path, relative_path) = split_absolute_path(path);
    let anchor = cap_primitives::fs::open_ambient_dir(&anchor_path, ambient_authority())?;
    if relative_path.as_os_str().is_empty() {
        Ok(anchor)
    } else {
        cap_primitives::fs::open_dir_nofollow(&anchor, relative_path)
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn split_absolute_path(path: &Path) -> (PathBuf, &Path) {
    let mut components = path.components();
    let mut anchor = PathBuf::new();
    while matches!(
        components.clone().next(),
        Some(Component::Prefix(_) | Component::RootDir)
    ) {
        let component = components
            .next()
            .expect("the preceding component check guarantees a component");
        anchor.push(component.as_os_str());
    }
    (anchor, components.as_path())
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn same_file(validated: &fs::Metadata, opened: &fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;

    validated.dev() == opened.dev() && validated.ino() == opened.ino()
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn safe_relative_file_path(path: &Path) -> bool {
    let mut saw_component = false;
    for component in path.components() {
        if !matches!(component, Component::Normal(_)) {
            return false;
        }
        saw_component = true;
    }
    saw_component
}
