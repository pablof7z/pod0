use std::fs;
use std::path::Path;

use pod0_domain::ContentDigest;

use crate::chapter_legacy_backup::{database_backup_path, evidence_backup_path};
use crate::chapter_rollback_database::verify_replayable_database;
use crate::chapter_rollback_format::ChapterRollbackManifest;
use crate::chapter_rollback_manifest::parse_digest;
use crate::transcript_import_digest::{TranscriptImportHash, digest_bytes, hex_digest};
use crate::{ChapterImportReport, StorageError};

pub(crate) const MANIFEST_FILE: &str = "manifest.json";
pub(crate) const DIGEST_FILE: &str = "bundle.digest";

pub(crate) fn verify_bundle(
    bundle: &Path,
    backup_root: &Path,
    report: &ChapterImportReport,
    expected: &ChapterRollbackManifest,
    expected_bytes: &[u8],
) -> Result<ContentDigest, StorageError> {
    let bytes = fs::read(bundle.join(MANIFEST_FILE))
        .map_err(|error| StorageError::io("read chapter rollback manifest", error))?;
    let decoded: ChapterRollbackManifest =
        serde_json::from_slice(&bytes).map_err(|_| StorageError::BackupConflict)?;
    if decoded != *expected || bytes != expected_bytes {
        return Err(StorageError::BackupConflict);
    }
    compare_file(
        &bundle.join(&expected.original_database_path),
        &database_backup_path(backup_root, &report.plan),
    )?;
    for entry in &expected.entries {
        let digest = parse_digest(&entry.raw_digest)?;
        let path = bundle.join(&entry.relative_path);
        verify_file(&path, digest, entry.raw_byte_count)?;
        compare_file(&path, &evidence_backup_path(backup_root, digest))?;
    }
    verify_replayable_database(bundle, expected)?;
    let digest = bundle_digest(bundle, expected, expected_bytes)?;
    let recorded = fs::read_to_string(bundle.join(DIGEST_FILE))
        .map_err(|error| StorageError::io("read chapter rollback digest", error))?;
    if recorded.trim() != hex_digest(digest) {
        return Err(StorageError::BackupConflict);
    }
    Ok(digest)
}

pub(crate) fn bundle_digest(
    root: &Path,
    manifest: &ChapterRollbackManifest,
    manifest_bytes: &[u8],
) -> Result<ContentDigest, StorageError> {
    let mut hash = TranscriptImportHash::new(b"pod0.chapter-rollback-bundle.v1");
    hash.bytes(manifest_bytes);
    for database in [&manifest.original_database_path, &manifest.database_path] {
        let bytes = fs::read(root.join(database))
            .map_err(|error| StorageError::io("read chapter rollback database", error))?;
        hash.bytes(&digest_bytes(&bytes).into_bytes());
    }
    let mut paths = manifest
        .entries
        .iter()
        .map(|entry| entry.relative_path.as_str())
        .collect::<Vec<_>>();
    paths.sort_unstable();
    paths.dedup();
    for path in paths {
        let bytes = fs::read(root.join(path))
            .map_err(|error| StorageError::io("read chapter rollback evidence", error))?;
        hash.text(path);
        hash.bytes(&digest_bytes(&bytes).into_bytes());
    }
    Ok(hash.finish())
}

fn verify_file(
    path: &Path,
    expected_digest: ContentDigest,
    expected_bytes: u64,
) -> Result<(), StorageError> {
    let bytes = fs::read(path)
        .map_err(|error| StorageError::io("read chapter rollback evidence", error))?;
    if bytes.len() as u64 != expected_bytes || digest_bytes(&bytes) != expected_digest {
        Err(StorageError::BackupConflict)
    } else {
        Ok(())
    }
}

fn compare_file(left: &Path, right: &Path) -> Result<(), StorageError> {
    let left = fs::read(left).map_err(|error| StorageError::io("read rollback file", error))?;
    let right = fs::read(right).map_err(|error| StorageError::io("read backup file", error))?;
    if left == right {
        Ok(())
    } else {
        Err(StorageError::BackupConflict)
    }
}
