use std::path::Path;

use pod0_domain::{ClipRecord, CommandId, StateRevision};
use rusqlite::{Transaction, TransactionBehavior, params};

use crate::clip_import_store_support::{
    clip_count, clip_import_count, current_core_revision, cutover_state, require_prerequisites,
    stored_clip_import_report,
};
use crate::clip_store_codec::encode_source;
use crate::clip_store_read::read_clip_snapshot;
use crate::legacy_clip_source::{digest, inspect_clip_source};
use crate::migration_db::{configure, open_connection, user_version};
use crate::{
    CURRENT_SCHEMA_VERSION, ClipBackupEvidence, ClipImportReport, ClipImportVerification,
    InspectedClipSource, StorageError,
};

pub(crate) fn write_clip_import<F>(
    target_path: &Path,
    import_id: CommandId,
    source: &InspectedClipSource,
    backup: &ClipBackupEvidence,
    verified_at_ms: i64,
    before_commit: F,
) -> Result<ClipImportReport, StorageError>
where
    F: FnOnce() -> Result<(), StorageError>,
{
    let mut connection = open_connection(target_path, false)?;
    configure(&connection)?;
    let version = user_version(&connection)?;
    if version != CURRENT_SCHEMA_VERSION {
        return Err(StorageError::CorruptSchema {
            detail: "clip import target is not at the current schema",
        });
    }
    crate::schema::validate_schema(&connection, version)?;
    require_prerequisites(&connection)?;
    if let Some(existing) = stored_clip_import_report(&connection, import_id, Some(backup))? {
        if existing.staged
            && existing.plan == source.plan
            && read_clip_snapshot(&connection)?.clips == source.clips
        {
            return Ok(ClipImportReport {
                reused_existing: true,
                ..existing
            });
        }
        return Err(StorageError::ImportConflict);
    }
    if clip_import_count(&connection)? != 0 || clip_count(&connection)? != 0 {
        return Err(StorageError::ImportConflict);
    }
    if cutover_state(&connection)?.is_some() {
        return Err(StorageError::ImportConflict);
    }
    let revision = current_core_revision(&connection)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| StorageError::sqlite("begin clip import", error))?;
    insert_import(
        &transaction,
        import_id,
        source,
        backup,
        revision,
        verified_at_ms,
    )?;
    insert_clips(&transaction, import_id, &source.clips)?;
    transaction
        .execute(
            "INSERT INTO pod0_clip_state(singleton,collection_revision,source_import_id) \
             VALUES(1,?1,?2)",
            params![
                i64::try_from(revision.value)
                    .map_err(|_| corrupt("clip import revision is malformed"))?,
                import_id.into_bytes().as_slice(),
            ],
        )
        .map_err(|error| StorageError::sqlite("initialize clip state", error))?;
    transaction
        .execute(
            "INSERT INTO pod0_domain_cutovers(domain,state,source_generation,core_revision,\
             committed_at_ms) VALUES('clips','staged',?1,?2,?3)",
            params![
                i64::try_from(source.plan.source_generation).map_err(|_| {
                    StorageError::ImportLimitExceeded {
                        entity: "clip source generation",
                    }
                })?,
                i64::try_from(revision.value)
                    .map_err(|_| corrupt("clip cutover revision is malformed"))?,
                verified_at_ms,
            ],
        )
        .map_err(|error| StorageError::sqlite("stage clip cutover", error))?;
    if read_clip_snapshot(&transaction)?.clips != source.clips {
        return Err(corrupt("staged clip projection differs from source"));
    }
    before_commit()?;
    transaction
        .commit()
        .map_err(|error| StorageError::sqlite("commit clip import", error))?;
    Ok(ClipImportReport {
        import_id,
        plan: source.plan.clone(),
        target_revision: revision,
        backup: backup.clone(),
        staged: true,
        reused_existing: false,
    })
}

pub fn read_clip_import(
    target_path: &Path,
    import_id: CommandId,
) -> Result<ClipImportVerification, StorageError> {
    let connection = open_connection(target_path, true)?;
    let report = stored_clip_import_report(&connection, import_id, None)?
        .ok_or(StorageError::ImportNotFound)?;
    Ok(ClipImportVerification {
        report,
        snapshot: read_clip_snapshot(&connection)?,
    })
}

pub fn commit_clip_cutover(
    source_path: &Path,
    path: &Path,
    observed_at_ms: i64,
) -> Result<bool, StorageError> {
    let mut connection = open_connection(path, false)?;
    configure(&connection)?;
    let version = user_version(&connection)?;
    if version != CURRENT_SCHEMA_VERSION {
        return Err(StorageError::CorruptSchema {
            detail: "clip cutover target is not at the current schema",
        });
    }
    crate::schema::validate_schema(&connection, version)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| StorageError::sqlite("begin clip cutover", error))?;
    let already = match cutover_state(&transaction)?.as_deref() {
        Some("authoritative") => true,
        Some("staged") => {
            let import_bytes: Vec<u8> = transaction
                .query_row(
                    "SELECT source_import_id FROM pod0_clip_state WHERE singleton=1",
                    [],
                    |row| row.get(0),
                )
                .map_err(|error| StorageError::sqlite("read staged clip import", error))?;
            let import_id = crate::model::command_id(&import_bytes)?;
            let report = stored_clip_import_report(&transaction, import_id, None)?
                .ok_or(StorageError::ImportNotFound)?;
            let snapshot = read_clip_snapshot(&transaction)?;
            if snapshot.clips.len() != report.plan.clip_count as usize
                || digest(&snapshot.clips) != report.plan.source_hash
            {
                return Err(StorageError::CorruptSchema {
                    detail: "staged clip snapshot does not match its verified import",
                });
            }
            let current = inspect_clip_source(source_path)?;
            if current.plan.source_kind != report.plan.source_kind
                || current.plan.source_hash != report.plan.source_hash
                || current.clips != snapshot.clips
            {
                discard_staged_clip_import(&transaction)?;
                transaction
                    .commit()
                    .map_err(|error| StorageError::sqlite("discard stale clip import", error))?;
                return Err(StorageError::SourceChanged);
            }
            transaction
                .execute(
                    "UPDATE pod0_domain_cutovers SET state='authoritative',committed_at_ms=?1 \
                     WHERE domain='clips' AND state='staged'",
                    [observed_at_ms],
                )
                .map_err(|error| StorageError::sqlite("commit clip cutover", error))?;
            false
        }
        _ => return Err(StorageError::ImportNotFound),
    };
    transaction
        .commit()
        .map_err(|error| StorageError::sqlite("commit clip cutover", error))?;
    Ok(already)
}

fn discard_staged_clip_import(transaction: &Transaction<'_>) -> Result<(), StorageError> {
    for (sql, operation) in [
        ("DELETE FROM pod0_clips", "discard staged clips"),
        ("DELETE FROM pod0_clip_state", "discard staged clip state"),
        (
            "DELETE FROM pod0_domain_cutovers WHERE domain='clips' AND state='staged'",
            "discard staged clip cutover",
        ),
        (
            "DELETE FROM pod0_clip_imports",
            "discard staged clip import",
        ),
    ] {
        transaction
            .execute(sql, [])
            .map_err(|error| StorageError::sqlite(operation, error))?;
    }
    Ok(())
}

fn insert_import(
    transaction: &Transaction<'_>,
    import_id: CommandId,
    source: &InspectedClipSource,
    backup: &ClipBackupEvidence,
    revision: StateRevision,
    verified_at_ms: i64,
) -> Result<(), StorageError> {
    transaction
        .execute(
            "INSERT INTO pod0_clip_imports(import_id,source_kind,source_hash,source_generation,\
             clip_count,backup_byte_count,target_revision,state,verified_at_ms) \
             VALUES(?1,?2,?3,?4,?5,?6,?7,'verified',?8)",
            params![
                import_id.into_bytes().as_slice(),
                source.plan.source_kind.code(),
                source.plan.source_hash,
                i64::try_from(source.plan.source_generation).map_err(|_| {
                    StorageError::ImportLimitExceeded {
                        entity: "clip source generation",
                    }
                })?,
                source.plan.clip_count,
                i64::try_from(backup.byte_count).map_err(|_| {
                    StorageError::ImportLimitExceeded {
                        entity: "clip backup bytes",
                    }
                })?,
                i64::try_from(revision.value)
                    .map_err(|_| corrupt("clip import revision is malformed"))?,
                verified_at_ms,
            ],
        )
        .map_err(|error| StorageError::sqlite("record clip import", error))?;
    Ok(())
}

fn insert_clips(
    transaction: &Transaction<'_>,
    import_id: CommandId,
    clips: &[ClipRecord],
) -> Result<(), StorageError> {
    for clip in clips {
        let (source_code, source_wire) = encode_source(clip.source);
        transaction
            .execute(
                "INSERT INTO pod0_clips(clip_id,clip_revision,episode_id,podcast_id,start_ms,end_ms,\
                 created_at_ms,caption,speaker_id,speaker_label,frozen_transcript_text,source_code,\
                 source_wire_code,deleted,source_import_id,created_command_id) \
                 VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,NULL)",
                params![
                    clip.clip_id.into_bytes().as_slice(),
                    i64::try_from(clip.revision.value)
                        .map_err(|_| corrupt("clip revision is malformed"))?,
                    clip.episode_id.into_bytes().as_slice(),
                    clip.podcast_id.into_bytes().as_slice(),
                    i64::try_from(clip.start_milliseconds).map_err(|_| StorageError::InvalidClip)?,
                    i64::try_from(clip.end_milliseconds).map_err(|_| StorageError::InvalidClip)?,
                    clip.created_at.value,
                    clip.caption,
                    clip.speaker_id.map(|value| value.into_bytes().to_vec()),
                    clip.speaker_label,
                    clip.frozen_transcript_text,
                    source_code,
                    source_wire,
                    i64::from(clip.deleted),
                    import_id.into_bytes().as_slice(),
                ],
            )
            .map_err(|error| StorageError::sqlite("import clip", error))?;
    }
    Ok(())
}

fn corrupt(detail: &'static str) -> StorageError {
    StorageError::CorruptSchema { detail }
}
