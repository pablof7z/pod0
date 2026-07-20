use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use pod0_domain::ContentDigest;

use crate::legacy_chapter_db::{backup_sqlite, inspect_chapter_database_snapshot};
use crate::transcript_backup_atomic::publish_verified_noclobber;
use crate::transcript_import_digest::{digest_bytes, hex_digest};
use crate::{ChapterBackupEvidence, ChapterImportPlan, InspectedChapterSource, StorageError};

pub(crate) fn create_or_reuse_chapter_backups(
    source_database_path: &Path,
    backup_root: &Path,
    source: &InspectedChapterSource,
) -> Result<ChapterBackupEvidence, StorageError> {
    let database_path = database_backup_path(backup_root, &source.plan);
    reject_database_alias(source_database_path, &database_path)?;
    let reused_database = database_path.exists();
    if !reused_database {
        match publish_verified_noclobber(
            &database_path,
            |unpublished| backup_sqlite(source_database_path, unpublished),
            |unpublished| verify_database_backup(source_database_path, unpublished, &source.plan),
        ) {
            Ok(()) => {}
            Err(StorageError::BackupConflict) if database_path.exists() => {}
            Err(error) => return Err(error),
        }
    }
    verify_database_backup(source_database_path, &database_path, &source.plan)?;
    let database_byte_count = fs::metadata(&database_path)
        .map_err(|error| StorageError::io("read chapter backup database metadata", error))?
        .len();
    let mut unique = BTreeMap::new();
    for entry in &source.entries {
        unique.entry(entry.raw_digest).or_insert(&entry.raw_bytes);
    }
    let mut reused_files = 0_u32;
    let mut file_byte_count = 0_u64;
    for (digest, bytes) in &unique {
        let destination = evidence_backup_path(backup_root, *digest);
        let reused = destination.exists();
        if reused {
            reused_files =
                reused_files
                    .checked_add(1)
                    .ok_or(StorageError::ImportLimitExceeded {
                        entity: "chapter backup files",
                    })?;
        } else {
            match publish_verified_noclobber(
                &destination,
                |path| write_bytes(path, bytes),
                |path| verify_evidence_backup(path, *digest, bytes.len() as u64),
            ) {
                Ok(()) => {}
                Err(StorageError::BackupConflict) if destination.exists() => {}
                Err(error) => return Err(error),
            }
        }
        verify_evidence_backup(&destination, *digest, bytes.len() as u64)?;
        file_byte_count = file_byte_count.checked_add(bytes.len() as u64).ok_or(
            StorageError::ImportLimitExceeded {
                entity: "chapter backup bytes",
            },
        )?;
    }
    Ok(ChapterBackupEvidence {
        database_digest: source.plan.source_database_digest,
        database_byte_count,
        file_count: u32::try_from(unique.len()).map_err(|_| StorageError::ImportLimitExceeded {
            entity: "chapter backup files",
        })?,
        file_byte_count,
        reused_database,
        reused_files,
    })
}

pub(crate) fn verify_chapter_backups(
    source_identity_path: &Path,
    backup_root: &Path,
    source: &InspectedChapterSource,
    expected: &ChapterBackupEvidence,
) -> Result<(), StorageError> {
    let database_path = database_backup_path(backup_root, &source.plan);
    verify_database_backup(source_identity_path, &database_path, &source.plan)?;
    let database_bytes = fs::metadata(&database_path)
        .map_err(|error| StorageError::io("read chapter backup database metadata", error))?
        .len();
    if expected.database_digest != source.plan.source_database_digest
        || expected.database_byte_count != database_bytes
    {
        return Err(StorageError::BackupConflict);
    }
    let mut unique = BTreeMap::new();
    for entry in &source.entries {
        unique.insert(entry.raw_digest, entry.raw_byte_count);
    }
    let mut byte_count = 0_u64;
    for (digest, bytes) in &unique {
        verify_evidence_backup(&evidence_backup_path(backup_root, *digest), *digest, *bytes)?;
        byte_count = byte_count
            .checked_add(*bytes)
            .ok_or(StorageError::BackupConflict)?;
    }
    if expected.file_count != u32::try_from(unique.len()).unwrap_or(u32::MAX)
        || expected.file_byte_count != byte_count
    {
        return Err(StorageError::BackupConflict);
    }
    Ok(())
}

pub(crate) fn database_backup_path(root: &Path, plan: &ChapterImportPlan) -> PathBuf {
    root.join("database").join(format!(
        "{}-{}.sqlite",
        plan.source_generation,
        hex_digest(plan.source_database_digest)
    ))
}

pub(crate) fn evidence_backup_path(root: &Path, digest: ContentDigest) -> PathBuf {
    root.join("evidence")
        .join(format!("{}.json", hex_digest(digest)))
}

fn verify_database_backup(
    source_identity_path: &Path,
    backup_path: &Path,
    plan: &ChapterImportPlan,
) -> Result<(), StorageError> {
    let database = inspect_chapter_database_snapshot(source_identity_path, backup_path)?;
    if database.source_kind != plan.source_kind
        || database.source_generation != plan.source_generation
        || database.source_file_identity != plan.source_file_identity
        || database.source_database_digest != plan.source_database_digest
    {
        return Err(StorageError::BackupConflict);
    }
    Ok(())
}

fn verify_evidence_backup(
    path: &Path,
    digest: ContentDigest,
    byte_count: u64,
) -> Result<(), StorageError> {
    let bytes =
        fs::read(path).map_err(|error| StorageError::io("read chapter evidence backup", error))?;
    if bytes.len() as u64 != byte_count || digest_bytes(&bytes) != digest {
        Err(StorageError::BackupConflict)
    } else {
        Ok(())
    }
}

fn write_bytes(path: &Path, bytes: &[u8]) -> Result<(), StorageError> {
    fs::write(path, bytes).map_err(|error| StorageError::io("write chapter evidence backup", error))
}

fn reject_database_alias(source: &Path, destination: &Path) -> Result<(), StorageError> {
    let source = fs::canonicalize(source)
        .map_err(|error| StorageError::io("resolve chapter source database", error))?;
    if destination.exists()
        && fs::canonicalize(destination)
            .map_err(|error| StorageError::io("resolve chapter backup database", error))?
            == source
    {
        Err(StorageError::BackupConflict)
    } else {
        Ok(())
    }
}
