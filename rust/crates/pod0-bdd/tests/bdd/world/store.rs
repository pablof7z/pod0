//! Production bootstrap seam used by every BDD scenario.

use std::path::PathBuf;

use pod0_facade::Pod0Facade;

/// Create the same empty authoritative store a fresh app installation uses.
pub(super) fn prepare_authoritative_store(directory: &tempfile::TempDir) -> PathBuf {
    let target = directory.path().join("core.sqlite");
    drop(
        Pod0Facade::create(target.to_string_lossy().into_owned())
            .expect("pod0-bdd: the fresh authoritative store must be creatable"),
    );
    target
}
