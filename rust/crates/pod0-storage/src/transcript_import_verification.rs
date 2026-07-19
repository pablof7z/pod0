use std::path::Path;

use pod0_domain::CommandId;
use rusqlite::{TransactionBehavior, params};

use crate::StorageError;
use crate::migration_db::configure;
use crate::transcript_import_digest::TranscriptImportHash;
use crate::transcript_import_model::{
    StoredTranscriptImportEntry, TranscriptImportState, TranscriptImportVerification,
};
use crate::transcript_import_store_read::{open_current, read_import_entries, read_import_report};
use crate::transcript_legacy_backup::{StoredBackupEntry, verify_transcript_backups};
use crate::transcript_store_read_artifact::read_artifact_by_id;

pub(crate) fn verify_transcript_import(
    target_path: &Path,
    backup_root: &Path,
    import_id: CommandId,
    verified_at_ms: i64,
) -> Result<TranscriptImportVerification, StorageError> {
    if verified_at_ms < 0 {
        return Err(StorageError::TranscriptImportConflict);
    }
    match verify_inner(target_path, backup_root, import_id, verified_at_ms) {
        Ok(verification) => Ok(verification),
        Err(error) => {
            mark_corrupt(target_path, import_id, error.code());
            Err(error)
        }
    }
}

fn verify_inner(
    target_path: &Path,
    backup_root: &Path,
    import_id: CommandId,
    verified_at_ms: i64,
) -> Result<TranscriptImportVerification, StorageError> {
    let mut connection = open_current(target_path)?;
    let report = read_import_report(&connection, import_id, true)?
        .ok_or(StorageError::TranscriptImportNotFound)?;
    if matches!(
        report.state,
        TranscriptImportState::Corrupt | TranscriptImportState::Discarded
    ) {
        return Err(StorageError::TranscriptImportConflict);
    }
    let entries = read_import_entries(&connection, import_id)?;
    if stored_selection_digest(report.plan.source_database_digest, &entries)
        != report.plan.source_selection_digest
    {
        return Err(StorageError::InvalidTranscriptArtifact);
    }
    let backups = entries
        .iter()
        .map(|entry| StoredBackupEntry {
            episode_id: entry.episode_id,
            file_digest: entry.backup_file_digest,
            byte_count: entry.backup_file_byte_count,
        })
        .collect::<Vec<_>>();
    verify_transcript_backups(
        backup_root,
        &report.plan,
        report.backup.database_digest,
        report.backup.database_byte_count,
        &backups,
    )?;
    let (segments, words) = verify_target_artifacts(&connection, &entries)?;
    if report.state == TranscriptImportState::Staged {
        configure(&connection)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| StorageError::sqlite("begin transcript import verification", error))?;
        let changed = transaction
            .execute(
                "UPDATE pod0_transcript_imports SET state='verified',verified_at_ms=?1,\
                 diagnostic_code=NULL WHERE import_id=?2 AND state='staged'",
                params![verified_at_ms, import_id.into_bytes().as_slice()],
            )
            .map_err(|error| StorageError::sqlite("verify transcript import", error))?;
        if changed != 1 {
            return Err(StorageError::TranscriptImportConflict);
        }
        transaction.commit().map_err(|error| {
            StorageError::sqlite("commit transcript import verification", error)
        })?;
    }
    let report = read_import_report(&connection, import_id, true)?
        .ok_or(StorageError::TranscriptImportNotFound)?;
    Ok(TranscriptImportVerification {
        verified_artifact_count: report.plan.artifact_count,
        verified_segment_count: segments,
        verified_word_count: words,
        report,
    })
}

fn verify_target_artifacts(
    connection: &rusqlite::Connection,
    entries: &[StoredTranscriptImportEntry],
) -> Result<(u64, u64), StorageError> {
    let mut segment_count = 0_u64;
    let mut word_count = 0_u64;
    for entry in entries {
        if entry.backup_file_digest != entry.selected_file_digest {
            return Err(StorageError::InvalidTranscriptArtifact);
        }
        let artifact = read_artifact_by_id(connection, entry.artifact_id)?
            .ok_or(StorageError::InvalidTranscriptArtifact)?;
        if artifact.episode_id != entry.episode_id
            || artifact.transcript_version_id != entry.transcript_version_id
            || artifact.provenance.source_payload_digest != entry.selected_file_digest
        {
            return Err(StorageError::InvalidTranscriptArtifact);
        }
        segment_count = segment_count
            .checked_add(artifact.segments.len() as u64)
            .ok_or(StorageError::InvalidTranscriptArtifact)?;
        word_count = artifact
            .segments
            .iter()
            .try_fold(word_count, |total, segment| {
                total.checked_add(segment.words.len() as u64)
            })
            .ok_or(StorageError::InvalidTranscriptArtifact)?;
    }
    Ok((segment_count, word_count))
}

fn stored_selection_digest(
    database_digest: pod0_domain::ContentDigest,
    entries: &[StoredTranscriptImportEntry],
) -> pod0_domain::ContentDigest {
    let mut hash = TranscriptImportHash::new(b"pod0.legacy-transcript-selection.v1");
    hash.bytes(&database_digest.into_bytes());
    hash.u64(entries.len() as u64);
    for entry in entries {
        hash.bytes(&entry.selected_row_digest.into_bytes());
        hash.bytes(&entry.selected_file_digest.into_bytes());
        hash.u64(entry.backup_file_byte_count);
        hash.bytes(&entry.artifact_id.into_bytes());
        hash.bytes(&entry.transcript_version_id.into_bytes());
    }
    hash.finish()
}

fn mark_corrupt(target_path: &Path, import_id: CommandId, diagnostic: &'static str) {
    let Ok(mut connection) = open_current(target_path) else {
        return;
    };
    if configure(&connection).is_err() {
        return;
    }
    let Ok(transaction) = connection.transaction_with_behavior(TransactionBehavior::Immediate)
    else {
        return;
    };
    let changed = transaction.execute(
        "UPDATE pod0_transcript_imports SET state='corrupt',diagnostic_code=?1,\
         verified_at_ms=NULL,committed_at_ms=NULL,discarded_at_ms=NULL \
         WHERE import_id=?2 AND state IN ('staged','verified')",
        params![diagnostic, import_id.into_bytes().as_slice()],
    );
    if changed.is_ok() {
        let _ = transaction.commit();
    }
}
