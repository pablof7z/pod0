use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};

use pod0_domain::{ContentDigest, EpisodeId};
use rusqlite::{Connection, MAIN_DB, OpenFlags};
use sha2::{Digest as _, Sha256};

use crate::StorageError;
use crate::legacy_transcript_db::inspect_legacy_transcript_database;
use crate::transcript_backup_atomic::publish_verified_noclobber;
use crate::transcript_import_digest::{digest_bytes, hex_digest};
use crate::transcript_import_model::{
    InspectedTranscriptEntry, InspectedTranscriptSource, TranscriptBackupEvidence,
    TranscriptImportPlan,
};

pub(crate) fn create_or_reuse_transcript_backups(
    source_database_path: &Path,
    backup_root: &Path,
    source: &InspectedTranscriptSource,
) -> Result<TranscriptBackupEvidence, StorageError> {
    fs::create_dir_all(backup_root)
        .map_err(|error| StorageError::io("create transcript backup root", error))?;
    let database_path = database_backup_path(backup_root, &source.plan);
    reject_alias(source_database_path, &database_path)?;
    let reused_database = database_path.exists();
    if !reused_database {
        publish_verified_noclobber(
            &database_path,
            |unpublished| backup_sqlite(source_database_path, unpublished),
            |unpublished| verify_database_backup(unpublished, &source.plan),
        )?;
    }
    verify_database_backup(&database_path, &source.plan)?;
    let (database_digest, database_byte_count) = digest_file(&database_path)?;

    let mut artifact_byte_count = 0_u64;
    let mut reused_artifacts = 0_u32;
    for entry in &source.entries {
        let destination =
            artifact_backup_path(backup_root, entry.episode_id, entry.selected_file_digest);
        reject_alias(&entry.selected_file_path, &destination)?;
        let reused = destination.exists();
        if reused {
            reused_artifacts =
                reused_artifacts
                    .checked_add(1)
                    .ok_or(StorageError::ImportLimitExceeded {
                        entity: "transcript backup count",
                    })?;
        } else {
            publish_verified_noclobber(
                &destination,
                |unpublished| copy_file(&entry.selected_file_path, unpublished),
                |unpublished| verify_artifact_backup(unpublished, entry),
            )?;
        }
        verify_artifact_backup(&destination, entry)?;
        artifact_byte_count = artifact_byte_count
            .checked_add(entry.selected_file_byte_count)
            .ok_or(StorageError::ImportLimitExceeded {
                entity: "transcript backup bytes",
            })?;
    }
    Ok(TranscriptBackupEvidence {
        database_digest,
        database_byte_count,
        artifact_count: source.plan.artifact_count,
        artifact_byte_count,
        reused_database,
        reused_artifacts,
    })
}

pub(crate) fn verify_transcript_backups(
    backup_root: &Path,
    plan: &TranscriptImportPlan,
    database_digest: ContentDigest,
    database_byte_count: u64,
    entries: &[StoredBackupEntry],
) -> Result<TranscriptBackupEvidence, StorageError> {
    let database_path = database_backup_path(backup_root, plan);
    verify_database_backup(&database_path, plan)?;
    let (stored_database_digest, stored_database_byte_count) = digest_file(&database_path)?;
    if stored_database_byte_count != database_byte_count
        || stored_database_digest != database_digest
    {
        return Err(StorageError::BackupConflict);
    }
    let mut artifact_byte_count = 0_u64;
    for entry in entries {
        let path = artifact_backup_path(backup_root, entry.episode_id, entry.file_digest);
        let bytes = read_bounded(&path, entry.byte_count, "read transcript artifact backup")?;
        if bytes.len() as u64 != entry.byte_count || digest_bytes(&bytes) != entry.file_digest {
            return Err(StorageError::BackupConflict);
        }
        artifact_byte_count = artifact_byte_count.checked_add(entry.byte_count).ok_or(
            StorageError::ImportLimitExceeded {
                entity: "transcript backup bytes",
            },
        )?;
    }
    Ok(TranscriptBackupEvidence {
        database_digest,
        database_byte_count,
        artifact_count: u32::try_from(entries.len()).map_err(|_| {
            StorageError::ImportLimitExceeded {
                entity: "transcript backup count",
            }
        })?,
        artifact_byte_count,
        reused_database: true,
        reused_artifacts: u32::try_from(entries.len()).unwrap_or(u32::MAX),
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct StoredBackupEntry {
    pub(crate) episode_id: EpisodeId,
    pub(crate) file_digest: ContentDigest,
    pub(crate) byte_count: u64,
}

pub(crate) fn database_backup_path(root: &Path, plan: &TranscriptImportPlan) -> PathBuf {
    root.join("database").join(format!(
        "{}-{}.sqlite",
        plan.source_generation,
        hex_digest(plan.source_selection_digest)
    ))
}

pub(crate) fn artifact_backup_path(
    root: &Path,
    episode_id: EpisodeId,
    digest: ContentDigest,
) -> PathBuf {
    root.join("artifacts")
        .join(hex_id(episode_id.into_bytes()))
        .join(format!("{}.json", hex_digest(digest)))
}

fn backup_sqlite(source_path: &Path, backup_path: &Path) -> Result<(), StorageError> {
    let source = Connection::open_with_flags(
        source_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_FULL_MUTEX,
    )
    .map_err(|error| StorageError::sqlite("open transcript source for backup", error))?;
    source
        .backup(MAIN_DB, backup_path, None)
        .map_err(|error| StorageError::sqlite("create transcript database backup", error))
}

fn verify_database_backup(path: &Path, plan: &TranscriptImportPlan) -> Result<(), StorageError> {
    let database = inspect_legacy_transcript_database(path)?;
    if database.source_kind != plan.source_kind
        || database.source_generation != plan.source_generation
        || database.database_digest != plan.source_database_digest
        || database
            .rows
            .iter()
            .filter(|row| row.integrity == "available")
            .count() as u32
            != plan.artifact_count
    {
        return Err(StorageError::BackupConflict);
    }
    Ok(())
}

fn verify_artifact_backup(
    path: &Path,
    expected: &InspectedTranscriptEntry,
) -> Result<(), StorageError> {
    let bytes = read_bounded(
        path,
        expected.selected_file_byte_count,
        "read transcript artifact backup",
    )?;
    if bytes.len() as u64 != expected.selected_file_byte_count
        || digest_bytes(&bytes) != expected.selected_file_digest
    {
        return Err(StorageError::BackupConflict);
    }
    Ok(())
}

fn copy_file(source: &Path, destination: &Path) -> Result<(), StorageError> {
    fs::copy(source, destination)
        .map_err(|error| StorageError::io("copy transcript artifact backup", error))?;
    Ok(())
}

fn read_bounded(
    path: &Path,
    expected: u64,
    operation: &'static str,
) -> Result<Vec<u8>, StorageError> {
    let mut file = File::open(path).map_err(|error| StorageError::io(operation, error))?;
    let size = file
        .metadata()
        .map_err(|error| StorageError::io(operation, error))?
        .len();
    if expected != u64::MAX && size > expected {
        return Err(StorageError::BackupConflict);
    }
    let mut bytes = Vec::with_capacity(usize::try_from(size).unwrap_or(0));
    file.read_to_end(&mut bytes)
        .map_err(|error| StorageError::io(operation, error))?;
    Ok(bytes)
}

fn digest_file(path: &Path) -> Result<(ContentDigest, u64), StorageError> {
    const MAX_DATABASE_BACKUP_BYTES: u64 = 2 * 1_024 * 1_024 * 1_024;
    let mut file = File::open(path)
        .map_err(|error| StorageError::io("open transcript database backup", error))?;
    let size = file
        .metadata()
        .map_err(|error| StorageError::io("read transcript database backup metadata", error))?
        .len();
    if size > MAX_DATABASE_BACKUP_BYTES {
        return Err(StorageError::ImportLimitExceeded {
            entity: "transcript database backup bytes",
        });
    }
    let mut hash = Sha256::new();
    let mut buffer = [0_u8; 64 * 1_024];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|error| StorageError::io("hash transcript database backup", error))?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok((ContentDigest::from_bytes(hash.finalize().into()), size))
}

fn reject_alias(source_path: &Path, backup_path: &Path) -> Result<(), StorageError> {
    let source = fs::canonicalize(source_path)
        .map_err(|error| StorageError::io("resolve transcript source path", error))?;
    if backup_path.exists() {
        let backup = fs::canonicalize(backup_path)
            .map_err(|error| StorageError::io("resolve transcript backup path", error))?;
        if source == backup {
            return Err(StorageError::BackupConflict);
        }
    } else if source_path == backup_path {
        return Err(StorageError::BackupConflict);
    }
    Ok(())
}

fn hex_id(value: [u8; 16]) -> String {
    value.iter().map(|byte| format!("{byte:02x}")).collect()
}
