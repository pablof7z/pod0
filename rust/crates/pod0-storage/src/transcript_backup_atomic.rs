use std::io::ErrorKind;
use std::path::Path;

use tempfile::NamedTempFile;

use crate::StorageError;

pub(crate) fn publish_verified_noclobber<W, V>(
    destination: &Path,
    write: W,
    verify: V,
) -> Result<(), StorageError>
where
    W: FnOnce(&Path) -> Result<(), StorageError>,
    V: FnOnce(&Path) -> Result<(), StorageError>,
{
    let parent = destination.parent().ok_or(StorageError::BackupConflict)?;
    std::fs::create_dir_all(parent)
        .map_err(|error| StorageError::io("create transcript backup directory", error))?;
    let unpublished = NamedTempFile::new_in(parent)
        .map_err(|error| StorageError::io("create transcript backup staging file", error))?;
    write(unpublished.path())?;
    verify(unpublished.path())?;
    unpublished
        .as_file()
        .sync_all()
        .map_err(|error| StorageError::io("sync transcript backup staging file", error))?;
    match unpublished.persist_noclobber(destination) {
        Ok(_) => Ok(()),
        Err(error) if error.error.kind() == ErrorKind::AlreadyExists => {
            Err(StorageError::BackupConflict)
        }
        Err(error) => Err(StorageError::io(
            "publish transcript backup without overwrite",
            error.error,
        )),
    }
}
