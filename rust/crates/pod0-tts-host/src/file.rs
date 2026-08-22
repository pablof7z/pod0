use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use tokio::fs::File;

use crate::{
    FileError, FileErrorKind, FileOperation, InvalidRequestError, InvalidRequestReason, TtsError,
    error::InvalidRequestField,
};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(1);
const TEMP_ATTEMPTS: usize = 32;

pub(crate) struct TemporaryOutput {
    path: Option<PathBuf>,
    file: Option<File>,
}

pub(crate) struct CommittedOutput {
    temporary: TemporaryOutput,
    staged_path: PathBuf,
}

impl TemporaryOutput {
    pub(crate) fn file_mut(&mut self) -> &mut File {
        self.file.as_mut().expect("temporary output is open")
    }

    fn path(&self) -> &Path {
        self.path.as_deref().expect("temporary output has a path")
    }

    fn close(&mut self) {
        drop(self.file.take());
    }

    fn remove_best_effort(&mut self) {
        self.close();
        let Some(path) = self.path.as_deref() else {
            return;
        };
        if std::fs::remove_file(path).is_ok() {
            self.path = None;
        }
    }
}

impl Drop for TemporaryOutput {
    fn drop(&mut self) {
        self.remove_best_effort();
    }
}

pub(crate) fn create(staged_path: &Path) -> Result<TemporaryOutput, TtsError> {
    let (parent, file_name) = path_parts(staged_path)?;
    if staged_path
        .try_exists()
        .map_err(|error| TtsError::from_io(FileOperation::InspectOutput, &error))?
    {
        return Err(TtsError::File(FileError {
            operation: FileOperation::FinalizeOutput,
            kind: FileErrorKind::AlreadyExists,
        }));
    }
    for _ in 0..TEMP_ATTEMPTS {
        let path = temporary_path(parent, file_name);
        match std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(file) => {
                return Ok(TemporaryOutput {
                    path: Some(path),
                    file: Some(File::from_std(file)),
                });
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(TtsError::from_io(FileOperation::CreateTemporary, &error));
            }
        }
    }
    Err(TtsError::File(FileError {
        operation: FileOperation::CreateTemporary,
        kind: FileErrorKind::AlreadyExists,
    }))
}

pub(crate) async fn discard(mut temporary: TemporaryOutput) {
    temporary.close();
    let path = temporary.path().to_owned();
    if tokio::fs::remove_file(path).await.is_ok() {
        temporary.path = None;
    }
}

pub(crate) fn commit(
    mut temporary: TemporaryOutput,
    staged_path: &Path,
) -> Result<CommittedOutput, TtsError> {
    temporary.close();
    // A successful no-replace link is the publication commit point. Everything
    // after it is best-effort housekeeping and cannot turn the receipt into failure.
    std::fs::hard_link(temporary.path(), staged_path)
        .map_err(|error| TtsError::from_io(FileOperation::FinalizeOutput, &error))?;
    Ok(CommittedOutput {
        temporary,
        staged_path: staged_path.to_owned(),
    })
}

pub(crate) fn finish(mut committed: CommittedOutput) {
    let _ = sync_parent(&committed.staged_path);
    committed.temporary.remove_best_effort();
    let _ = sync_parent(&committed.staged_path);
}

fn path_parts(path: &Path) -> Result<(&Path, &std::ffi::OsStr), TtsError> {
    let parent = path.parent().map(|value| {
        if value.as_os_str().is_empty() {
            Path::new(".")
        } else {
            value
        }
    });
    let file_name = path.file_name().filter(|value| !value.is_empty());
    match (parent, file_name) {
        (Some(parent), Some(file_name)) => Ok((parent, file_name)),
        _ => Err(TtsError::InvalidRequest(InvalidRequestError {
            field: InvalidRequestField::StagedPath,
            reason: InvalidRequestReason::Invalid,
        })),
    }
}

fn temporary_path(parent: &Path, file_name: &std::ffi::OsStr) -> PathBuf {
    let mut name = OsString::from(".");
    name.push(file_name);
    name.push(format!(
        ".pod0-tts-part-{}-{}",
        std::process::id(),
        TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    parent.join(name)
}

#[cfg(unix)]
fn sync_parent(path: &Path) -> Result<(), TtsError> {
    let (parent, _) = path_parts(path)?;
    std::fs::File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| TtsError::from_io(FileOperation::SyncDirectory, &error))
}

#[cfg(not(unix))]
fn sync_parent(_path: &Path) -> Result<(), TtsError> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use tokio::io::AsyncWriteExt as _;

    use super::*;

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

    #[tokio::test]
    async fn cleanup_failure_after_commit_does_not_negate_publication() {
        let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(".test-artifacts")
            .join(format!(
                "commit-cleanup-{}-{}",
                std::process::id(),
                TEST_COUNTER.fetch_add(1, Ordering::Relaxed)
            ));
        std::fs::create_dir_all(&directory).unwrap();
        let staged_path = directory.join("audio.mp3");
        let mut temporary = create(&staged_path).unwrap();
        temporary.file_mut().write_all(b"audio").await.unwrap();
        temporary.file_mut().sync_all().await.unwrap();

        let committed = commit(temporary, &staged_path).unwrap();
        std::fs::remove_file(committed.temporary.path()).unwrap();
        finish(committed);

        assert_eq!(std::fs::read(&staged_path).unwrap(), b"audio");
        std::fs::remove_dir_all(directory).unwrap();
    }
}
