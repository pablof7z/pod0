use std::ffi::OsString;
use std::path::{Path, PathBuf};

use crate::{
    RECALL_INDEX_SCHEMA_VERSION, RecallIndex, RecallIndexCutoverReceipt, RecallIndexError,
};

impl RecallIndex {
    pub fn commit_legacy_cutover(
        &self,
        removed_legacy_file_count: u8,
    ) -> Result<RecallIndexCutoverReceipt, RecallIndexError> {
        if removed_legacy_file_count > 3 {
            return Err(RecallIndexError::InvalidInput(
                "legacy recall artifact count is invalid",
            ));
        }
        self.connection.execute(
            "UPDATE pod0_recall_index_metadata
             SET legacy_cutover_committed=1 WHERE singleton=1",
            [],
        )?;
        Ok(RecallIndexCutoverReceipt {
            schema_version: RECALL_INDEX_SCHEMA_VERSION,
            removed_legacy_file_count,
        })
    }

    pub fn legacy_cutover_is_committed(&self) -> Result<bool, RecallIndexError> {
        let value: i64 = self.connection.query_row(
            "SELECT legacy_cutover_committed
             FROM pod0_recall_index_metadata WHERE singleton=1",
            [],
            |row| row.get(0),
        )?;
        Ok(value == 1)
    }
}

pub(crate) fn validate_disposable_artifacts(path: &Path) -> Result<(), RecallIndexError> {
    for artifact in disposable_artifacts(path) {
        match artifact.symlink_metadata() {
            Ok(metadata) if metadata.file_type().is_file() => {}
            Ok(_) => {
                return Err(RecallIndexError::InvalidInput(
                    "recall index artifact is not a regular file",
                ));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

pub(crate) fn remove_disposable_artifacts(path: &Path) -> Result<u8, RecallIndexError> {
    validate_disposable_artifacts(path)?;
    let mut removed = 0_u8;
    for artifact in disposable_artifacts(path) {
        match artifact.symlink_metadata() {
            Ok(metadata) if metadata.file_type().is_file() => {
                std::fs::remove_file(artifact)?;
                removed = removed.saturating_add(1);
            }
            Ok(_) => {
                return Err(RecallIndexError::InvalidInput(
                    "recall index artifact changed during removal",
                ));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(removed)
}

fn disposable_artifacts(path: &Path) -> [PathBuf; 3] {
    [
        path.to_path_buf(),
        suffixed(path, "-wal"),
        suffixed(path, "-shm"),
    ]
}

fn suffixed(path: &Path, suffix: &str) -> PathBuf {
    let mut value = OsString::from(path.as_os_str());
    value.push(suffix);
    PathBuf::from(value)
}
