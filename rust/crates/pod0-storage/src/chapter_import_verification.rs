use std::path::Path;

use pod0_domain::CommandId;
use rusqlite::{TransactionBehavior, params};

use crate::chapter_import_identity_verification::verify_import_identity_evidence;
use crate::chapter_import_store_read::{open_current, read_import_entries, read_import_report};
use crate::chapter_legacy_backup::verify_chapter_backups;
use crate::chapter_store_read_artifact::read_chapter_artifact;
use crate::legacy_chapter_source::inspect_chapter_source;
use crate::{ChapterImportState, ChapterImportVerification, StorageError};

pub(crate) fn verify_chapter_import(
    source_database_path: &Path,
    artifact_root: &Path,
    backup_root: &Path,
    target_path: &Path,
    import_id: CommandId,
    verified_at_ms: i64,
) -> Result<ChapterImportVerification, StorageError> {
    verify_chapter_import_with_observer(
        source_database_path,
        artifact_root,
        backup_root,
        target_path,
        import_id,
        verified_at_ms,
        || Ok(()),
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn verify_chapter_import_with_observer<F>(
    source_database_path: &Path,
    artifact_root: &Path,
    backup_root: &Path,
    target_path: &Path,
    import_id: CommandId,
    verified_at_ms: i64,
    before_commit: F,
) -> Result<ChapterImportVerification, StorageError>
where
    F: FnOnce() -> Result<(), StorageError>,
{
    if verified_at_ms < 0 {
        return Err(StorageError::ChapterImportConflict);
    }
    match verify_inner(
        source_database_path,
        artifact_root,
        backup_root,
        target_path,
        import_id,
        verified_at_ms,
        before_commit,
    ) {
        Ok(result) => Ok(result),
        Err(StorageError::Interrupted) => Err(StorageError::Interrupted),
        Err(error) => {
            mark_corrupt(target_path, import_id, error.code());
            Err(error)
        }
    }
}

fn verify_inner<F>(
    source_database_path: &Path,
    artifact_root: &Path,
    backup_root: &Path,
    target_path: &Path,
    import_id: CommandId,
    verified_at_ms: i64,
    before_commit: F,
) -> Result<ChapterImportVerification, StorageError>
where
    F: FnOnce() -> Result<(), StorageError>,
{
    let source = inspect_chapter_source(source_database_path, artifact_root)?;
    let mut connection = open_current(target_path)?;
    let report = read_import_report(&connection, import_id, true)?
        .ok_or(StorageError::ChapterImportNotFound)?;
    if report.plan != source.plan {
        return Err(StorageError::SourceChanged);
    }
    if matches!(
        report.state,
        ChapterImportState::Corrupt | ChapterImportState::Discarded
    ) {
        return Err(StorageError::ChapterImportConflict);
    }
    if source.plan.blocked_count > 0 {
        mark_corrupt(target_path, import_id, "blocked_legacy_evidence");
        return Err(StorageError::InvalidChapterArtifact);
    }
    verify_chapter_backups(source_database_path, backup_root, &source, &report.backup)?;
    verify_stored_evidence(&connection, import_id, &source)?;
    verify_import_identity_evidence(&connection, import_id, &source)?;
    let (chapters, ad_spans) = verify_artifacts(&connection, &source)?;
    if report.state == ChapterImportState::Staged {
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| StorageError::sqlite("begin chapter import verification", error))?;
        let changed = transaction
            .execute(
                "UPDATE pod0_chapter_imports SET state='verified',verified_at_ms=?1,\
                 diagnostic_code=NULL WHERE import_id=?2 AND state='staged'",
                params![verified_at_ms, import_id.into_bytes().as_slice()],
            )
            .map_err(|error| StorageError::sqlite("verify chapter import", error))?;
        if changed != 1 {
            return Err(StorageError::ChapterImportConflict);
        }
        before_commit()?;
        transaction
            .commit()
            .map_err(|error| StorageError::sqlite("commit chapter import verification", error))?;
    }
    let report = read_import_report(&connection, import_id, true)?
        .ok_or(StorageError::ChapterImportNotFound)?;
    Ok(ChapterImportVerification {
        verified_evidence_count: report.plan.evidence_count,
        verified_artifact_count: report.plan.canonical_artifact_count,
        verified_chapter_count: chapters,
        verified_ad_span_count: ad_spans,
        report,
    })
}

fn verify_stored_evidence(
    connection: &rusqlite::Connection,
    import_id: CommandId,
    source: &crate::InspectedChapterSource,
) -> Result<(), StorageError> {
    let stored = read_import_entries(connection, import_id)?;
    if stored.len() != source.entries.len() {
        return Err(StorageError::InvalidChapterArtifact);
    }
    for (stored, current) in stored.iter().zip(&source.entries) {
        if stored.evidence_id != current.evidence_id
            || stored.raw_digest != current.raw_digest
            || stored.raw_byte_count != current.raw_byte_count
            || stored.artifact_id
                != current
                    .artifact
                    .as_ref()
                    .map(|artifact| artifact.artifact_id)
            || stored.validation != current.validation
        {
            return Err(StorageError::InvalidChapterArtifact);
        }
    }
    Ok(())
}

fn verify_artifacts(
    connection: &rusqlite::Connection,
    source: &crate::InspectedChapterSource,
) -> Result<(u64, u64), StorageError> {
    let mut artifacts = std::collections::BTreeMap::new();
    for artifact in source
        .entries
        .iter()
        .filter_map(|entry| entry.artifact.as_ref())
    {
        artifacts.entry(artifact.artifact_id).or_insert(artifact);
    }
    let mut chapters = 0_u64;
    let mut ad_spans = 0_u64;
    for (id, expected) in artifacts {
        let stored =
            read_chapter_artifact(connection, id)?.ok_or(StorageError::InvalidChapterArtifact)?;
        if stored != *expected || stored.verify_integrity().is_err() {
            return Err(StorageError::InvalidChapterArtifact);
        }
        chapters = chapters
            .checked_add(stored.chapters.len() as u64)
            .ok_or(StorageError::InvalidChapterArtifact)?;
        ad_spans = ad_spans
            .checked_add(stored.ad_spans.len() as u64)
            .ok_or(StorageError::InvalidChapterArtifact)?;
    }
    Ok((chapters, ad_spans))
}

pub(crate) fn mark_corrupt(target_path: &Path, import_id: CommandId, diagnostic: &str) {
    let Ok(mut connection) = open_current(target_path) else {
        return;
    };
    let Ok(transaction) = connection.transaction_with_behavior(TransactionBehavior::Immediate)
    else {
        return;
    };
    let diagnostic = if diagnostic.is_empty() || diagnostic.len() > 128 {
        "chapter_import_corrupt"
    } else {
        diagnostic
    };
    let _ = transaction.execute(
        "UPDATE pod0_chapter_imports SET state='corrupt',diagnostic_code=?1 \
         WHERE import_id=?2 AND state IN ('staged','verified','corrupt')",
        params![diagnostic, import_id.into_bytes().as_slice()],
    );
    let _ = transaction.commit();
}
