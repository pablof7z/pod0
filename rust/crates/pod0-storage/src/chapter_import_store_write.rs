use std::collections::BTreeMap;
use std::path::Path;

use pod0_domain::{ChapterArtifactId, CommandId, ContentDigest, StateRevision};
use rusqlite::{OptionalExtension, TransactionBehavior};

use crate::chapter_import_store_read::{open_current, read_import_fingerprint, read_import_report};
use crate::chapter_import_store_rows::{insert_import_entry, insert_import_record};
use crate::chapter_store_write_artifact::insert_or_validate_chapter_artifact;
use crate::migration_db::configure;
use crate::transcript_import_digest::TranscriptImportHash;
use crate::{
    ChapterBackupEvidence, ChapterImportPlan, ChapterImportReport, InspectedChapterSource,
    StorageError,
};

pub(crate) fn chapter_import_fingerprint(
    import_id: CommandId,
    target_store_id: CommandId,
    plan: &ChapterImportPlan,
) -> ContentDigest {
    let mut hash = TranscriptImportHash::new(b"pod0.chapter-import-command.v1");
    hash.bytes(&import_id.into_bytes());
    hash.bytes(&target_store_id.into_bytes());
    hash.text(plan.source_kind.code());
    hash.u64(plan.source_generation);
    hash.bytes(&plan.source_file_identity.into_bytes());
    hash.u64(plan.source_database_byte_count);
    hash.bytes(&plan.source_database_digest.into_bytes());
    hash.bytes(&plan.source_selection_digest.into_bytes());
    hash.u32(plan.evidence_count);
    hash.u32(plan.canonical_artifact_count);
    hash.u32(plan.selected_count);
    hash.u32(plan.blocked_count);
    hash.finish()
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn write_chapter_import<F>(
    target_path: &Path,
    import_id: CommandId,
    target_store_id: CommandId,
    source: &InspectedChapterSource,
    backup: &ChapterBackupEvidence,
    staged_at_ms: i64,
    before_commit: F,
) -> Result<ChapterImportReport, StorageError>
where
    F: FnOnce() -> Result<(), StorageError>,
{
    if staged_at_ms < 0 {
        return Err(StorageError::ChapterImportConflict);
    }
    let fingerprint = chapter_import_fingerprint(import_id, target_store_id, &source.plan);
    let mut connection = open_current(target_path)?;
    configure(&connection)?;
    if let Some(existing) = read_import_report(&connection, import_id, true)? {
        return if read_import_fingerprint(&connection, import_id)? == Some(fingerprint)
            && existing.plan == source.plan
        {
            Ok(existing)
        } else {
            Err(StorageError::ChapterImportConflict)
        };
    }
    let active: Option<Vec<u8>> = connection
        .query_row(
            "SELECT import_id FROM pod0_chapter_imports \
             WHERE state IN ('staged','verified','corrupt') LIMIT 1",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read active chapter import", error))?;
    if active.is_some() {
        return Err(StorageError::ChapterImportConflict);
    }
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| StorageError::sqlite("begin chapter import", error))?;
    require_authority_inactive(&transaction)?;
    let target_revision = next_target_revision(&transaction)?;
    let artifacts = canonical_artifacts(source)?;
    let chapter_count = artifacts
        .values()
        .try_fold(0_u64, |total, artifact| {
            total.checked_add(artifact.chapters.len() as u64)
        })
        .ok_or(StorageError::ChapterImportConflict)?;
    let ad_span_count = artifacts
        .values()
        .try_fold(0_u64, |total, artifact| {
            total.checked_add(artifact.ad_spans.len() as u64)
        })
        .ok_or(StorageError::ChapterImportConflict)?;
    insert_import_record(
        &transaction,
        import_id,
        fingerprint,
        source,
        backup,
        target_revision,
        chapter_count,
        ad_span_count,
        staged_at_ms,
    )?;
    for artifact in artifacts.values() {
        if artifact.podcast_id == crate::retained_orphan_parent::retained_orphan_podcast_id() {
            crate::retained_orphan_parent::ensure_retained_orphan_parent(
                &transaction,
                artifact.episode_id,
                staged_at_ms,
            )?;
        }
        insert_or_validate_chapter_artifact(&transaction, artifact, Some(import_id), staged_at_ms)?;
    }
    for entry in &source.entries {
        insert_import_entry(&transaction, import_id, source.plan.source_kind, entry)?;
    }
    before_commit()?;
    transaction
        .commit()
        .map_err(|error| StorageError::sqlite("commit chapter import stage", error))?;
    let mut report = read_import_report(&connection, import_id, false)?
        .ok_or(StorageError::ChapterImportNotFound)?;
    report.backup.reused_database = backup.reused_database;
    report.backup.reused_files = backup.reused_files;
    Ok(report)
}

fn canonical_artifacts(
    source: &InspectedChapterSource,
) -> Result<BTreeMap<ChapterArtifactId, &pod0_domain::ChapterArtifact>, StorageError> {
    let mut artifacts = BTreeMap::new();
    for artifact in source
        .entries
        .iter()
        .filter_map(|entry| entry.artifact.as_ref())
    {
        if let Some(existing) = artifacts.insert(artifact.artifact_id, artifact)
            && existing != artifact
        {
            return Err(StorageError::InvalidChapterArtifact);
        }
    }
    if artifacts.len() != source.plan.canonical_artifact_count as usize {
        return Err(StorageError::ChapterImportConflict);
    }
    Ok(artifacts)
}

fn require_authority_inactive(transaction: &rusqlite::Transaction<'_>) -> Result<(), StorageError> {
    let state: (bool, Option<Vec<u8>>) = transaction
        .query_row(
            "SELECT authority_active,authority_import_id FROM pod0_chapter_state WHERE singleton=1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|error| StorageError::sqlite("read chapter authority", error))?;
    if state == (false, None) {
        Ok(())
    } else {
        Err(StorageError::CutoverAlreadyAuthoritative)
    }
}

fn next_target_revision(
    transaction: &rusqlite::Transaction<'_>,
) -> Result<StateRevision, StorageError> {
    let current: i64 = transaction
        .query_row(
            "SELECT collection_revision FROM pod0_chapter_state WHERE singleton=1",
            [],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("read chapter import revision", error))?;
    let next = u64::try_from(current)
        .ok()
        .and_then(|value| value.checked_add(1))
        .ok_or(StorageError::ChapterImportConflict)?;
    Ok(StateRevision::new(next))
}
