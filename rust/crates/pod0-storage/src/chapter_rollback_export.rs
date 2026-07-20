use std::collections::BTreeSet;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use pod0_domain::{CommandId, ContentDigest};
use rusqlite::OptionalExtension;

use crate::chapter_import_store_read::{open_current, read_import_report};
use crate::chapter_legacy_backup::{database_backup_path, evidence_backup_path};
use crate::chapter_rollback_database::make_replayable_database;
use crate::chapter_rollback_format::ChapterRollbackManifest;
use crate::chapter_rollback_manifest::{build_manifest, parse_digest};
use crate::chapter_rollback_verify::{DIGEST_FILE, MANIFEST_FILE, bundle_digest, verify_bundle};
use crate::transcript_import_digest::hex_digest;
use crate::{
    CURRENT_SCHEMA_VERSION, ChapterImportReport, ChapterImportState, ChapterRollbackExportReport,
    StorageError,
};

pub const CHAPTER_ROLLBACK_FORMAT_VERSION: u32 = 1;

pub fn export_chapter_rollback_bundle(
    target_path: &Path,
    legacy_backup_root: &Path,
    export_root: &Path,
) -> Result<ChapterRollbackExportReport, StorageError> {
    export_chapter_rollback_bundle_with_observer(
        target_path,
        legacy_backup_root,
        export_root,
        || Ok(()),
    )
}

pub(crate) fn export_chapter_rollback_bundle_with_observer<F>(
    target_path: &Path,
    legacy_backup_root: &Path,
    export_root: &Path,
    before_publish: F,
) -> Result<ChapterRollbackExportReport, StorageError>
where
    F: FnOnce() -> Result<(), StorageError>,
{
    let connection = open_current(target_path)?;
    let import_id = latest_imported_id(&connection)?;
    let report = read_import_report(&connection, import_id, true)?
        .ok_or(StorageError::ChapterImportNotFound)?;
    if report.state != ChapterImportState::Imported {
        return Err(StorageError::ChapterImportConflict);
    }
    let manifest = build_manifest(&connection, &report)?;
    let manifest_bytes =
        serde_json::to_vec_pretty(&manifest).map_err(|_| StorageError::InvalidChapterArtifact)?;
    fs::create_dir_all(export_root)
        .map_err(|error| StorageError::io("create chapter rollback root", error))?;
    let export_root = fs::canonicalize(export_root)
        .map_err(|error| StorageError::io("resolve chapter rollback root", error))?;
    let bundle_path = export_root.join(format!(
        "chapters-v{}-core-v{}-generation-{}-{}",
        CHAPTER_ROLLBACK_FORMAT_VERSION,
        CURRENT_SCHEMA_VERSION,
        report.plan.source_generation,
        hex_digest(report.plan.source_selection_digest)
    ));
    if bundle_path.exists() {
        let digest = verify_bundle(
            &bundle_path,
            legacy_backup_root,
            &report,
            &manifest,
            &manifest_bytes,
        )?;
        return Ok(export_report(bundle_path, &report, digest, true));
    }
    let staging = tempfile::Builder::new()
        .prefix(".pod0-chapter-rollback-")
        .tempdir_in(&export_root)
        .map_err(|error| StorageError::io("stage chapter rollback export", error))?;
    write_bundle(
        staging.path(),
        legacy_backup_root,
        &report,
        &manifest,
        &manifest_bytes,
    )?;
    before_publish()?;
    let reused_existing = match fs::rename(staging.path(), &bundle_path) {
        Ok(()) => {
            sync_directory(&export_root)?;
            false
        }
        Err(_) if bundle_path.exists() => true,
        Err(error) => return Err(StorageError::io("publish chapter rollback export", error)),
    };
    let digest = verify_bundle(
        &bundle_path,
        legacy_backup_root,
        &report,
        &manifest,
        &manifest_bytes,
    )?;
    Ok(export_report(bundle_path, &report, digest, reused_existing))
}

fn write_bundle(
    staging: &Path,
    backup_root: &Path,
    report: &ChapterImportReport,
    manifest: &ChapterRollbackManifest,
    manifest_bytes: &[u8],
) -> Result<(), StorageError> {
    fs::create_dir(staging.join("evidence"))
        .map_err(|error| StorageError::io("create chapter rollback evidence directory", error))?;
    let backup = database_backup_path(backup_root, &report.plan);
    copy_synced(&backup, &staging.join(&manifest.original_database_path))?;
    copy_synced(&backup, &staging.join(&manifest.database_path))?;
    make_replayable_database(&staging.join(&manifest.database_path), manifest)?;
    let mut copied = BTreeSet::new();
    for entry in &manifest.entries {
        let digest = parse_digest(&entry.raw_digest)?;
        if copied.insert(digest) {
            copy_synced(
                &evidence_backup_path(backup_root, digest),
                &staging.join(&entry.relative_path),
            )?;
        }
    }
    write_synced(&staging.join(MANIFEST_FILE), manifest_bytes)?;
    let digest = bundle_digest(staging, manifest, manifest_bytes)?;
    write_synced(
        &staging.join(DIGEST_FILE),
        format!("{}\n", hex_digest(digest)).as_bytes(),
    )?;
    sync_directory(staging)
}

fn latest_imported_id(connection: &rusqlite::Connection) -> Result<CommandId, StorageError> {
    let bytes: Option<Vec<u8>> = connection
        .query_row(
            "SELECT import_id FROM pod0_chapter_imports WHERE state='imported' \
             ORDER BY imported_at_ms DESC,import_id DESC LIMIT 1",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read chapter rollback import", error))?;
    let value: [u8; 16] = bytes
        .ok_or(StorageError::ChapterImportNotFound)?
        .try_into()
        .map_err(|_| StorageError::BackupConflict)?;
    Ok(CommandId::from_bytes(value))
}

fn copy_synced(source: &Path, destination: &Path) -> Result<(), StorageError> {
    fs::copy(source, destination)
        .map_err(|error| StorageError::io("copy chapter rollback file", error))?;
    File::open(destination)
        .and_then(|file| file.sync_all())
        .map_err(|error| StorageError::io("sync chapter rollback file", error))
}

fn write_synced(path: &Path, bytes: &[u8]) -> Result<(), StorageError> {
    let mut file = File::create(path)
        .map_err(|error| StorageError::io("create chapter rollback file", error))?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|error| StorageError::io("write chapter rollback file", error))
}

fn sync_directory(path: &Path) -> Result<(), StorageError> {
    File::open(path)
        .and_then(|file| file.sync_all())
        .map_err(|error| StorageError::io("sync chapter rollback directory", error))
}

fn export_report(
    bundle_path: PathBuf,
    report: &ChapterImportReport,
    bundle_digest: ContentDigest,
    reused_existing: bool,
) -> ChapterRollbackExportReport {
    ChapterRollbackExportReport {
        bundle_path,
        format_version: CHAPTER_ROLLBACK_FORMAT_VERSION,
        core_schema_version: CURRENT_SCHEMA_VERSION,
        source_generation: report.plan.source_generation,
        evidence_count: report.plan.evidence_count,
        artifact_count: report.plan.canonical_artifact_count,
        bundle_digest,
        reused_existing,
    }
}
