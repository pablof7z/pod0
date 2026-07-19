use std::path::Path;

use pod0_domain::{CommandId, NoteRecord, StateRevision};
use rusqlite::{Transaction, TransactionBehavior, params};

use crate::migration_db::{configure, open_connection, user_version};
use crate::note_import_store_support::{
    current_core_revision, cutover_state, note_count, note_import_count,
    require_listening_authoritative, stored_note_import_report,
};
use crate::note_store_codec::{encode_author, encode_kind, encode_target};
use crate::note_store_read::read_note_snapshot;
use crate::{
    CURRENT_SCHEMA_VERSION, InspectedNoteSource, NoteBackupEvidence, NoteImportReport,
    NoteImportVerification, StorageError,
};

pub(crate) fn write_note_import<F>(
    target_path: &Path,
    import_id: CommandId,
    source: &InspectedNoteSource,
    backup: &NoteBackupEvidence,
    verified_at_ms: i64,
    before_commit: F,
) -> Result<NoteImportReport, StorageError>
where
    F: FnOnce() -> Result<(), StorageError>,
{
    let mut connection = open_connection(target_path, false)?;
    configure(&connection)?;
    let version = user_version(&connection)?;
    if version != CURRENT_SCHEMA_VERSION {
        return Err(StorageError::CorruptSchema {
            detail: "note import target is not at the current schema",
        });
    }
    crate::schema::validate_schema(&connection, version)?;
    require_listening_authoritative(&connection)?;
    if let Some(existing) = stored_note_import_report(&connection, import_id, Some(backup))? {
        if existing.staged
            && existing.plan == source.plan
            && read_note_snapshot(&connection)?.notes == source.notes
        {
            return Ok(NoteImportReport {
                reused_existing: true,
                ..existing
            });
        }
        return Err(StorageError::ImportConflict);
    }
    if note_import_count(&connection)? != 0 || note_count(&connection)? != 0 {
        return Err(StorageError::ImportConflict);
    }
    if cutover_state(&connection)?.is_some() {
        return Err(StorageError::ImportConflict);
    }
    let revision = current_core_revision(&connection)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| StorageError::sqlite("begin note import", error))?;
    insert_import(
        &transaction,
        import_id,
        source,
        backup,
        revision,
        verified_at_ms,
    )?;
    insert_notes(&transaction, import_id, &source.notes)?;
    transaction
        .execute(
            "INSERT INTO pod0_note_state(singleton,collection_revision,source_import_id) \
         VALUES(1,?1,?2)",
            params![
                i64::try_from(revision.value).map_err(|_| StorageError::CorruptSchema {
                    detail: "note import revision is malformed",
                })?,
                import_id.into_bytes().as_slice(),
            ],
        )
        .map_err(|error| StorageError::sqlite("initialize note state", error))?;
    transaction.execute(
        "INSERT INTO pod0_domain_cutovers(domain,state,source_generation,core_revision,committed_at_ms) \
         VALUES('notes','staged',?1,?2,?3)",
        params![
            i64::try_from(source.plan.source_generation).map_err(|_| StorageError::ImportLimitExceeded {
                entity: "note source generation",
            })?,
            i64::try_from(revision.value).map_err(|_| StorageError::CorruptSchema {
                detail: "note cutover revision is malformed",
            })?,
            verified_at_ms,
        ],
    ).map_err(|error| StorageError::sqlite("stage note cutover", error))?;
    if read_note_snapshot(&transaction)?.notes != source.notes {
        return Err(StorageError::CorruptSchema {
            detail: "staged note projection differs from source",
        });
    }
    before_commit()?;
    transaction
        .commit()
        .map_err(|error| StorageError::sqlite("commit note import", error))?;
    Ok(NoteImportReport {
        import_id,
        plan: source.plan.clone(),
        target_revision: revision,
        backup: backup.clone(),
        staged: true,
        reused_existing: false,
    })
}

pub fn read_note_import(
    target_path: &Path,
    import_id: CommandId,
) -> Result<NoteImportVerification, StorageError> {
    let connection = open_connection(target_path, true)?;
    let report = stored_note_import_report(&connection, import_id, None)?
        .ok_or(StorageError::ImportNotFound)?;
    Ok(NoteImportVerification {
        report,
        snapshot: read_note_snapshot(&connection)?,
    })
}

pub fn commit_note_cutover(path: &Path, observed_at_ms: i64) -> Result<bool, StorageError> {
    let mut connection = open_connection(path, false)?;
    configure(&connection)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| StorageError::sqlite("begin note cutover", error))?;
    let state = cutover_state(&transaction)?;
    let already = match state.as_deref() {
        Some("authoritative") => true,
        Some("staged") => {
            transaction
                .execute(
                    "UPDATE pod0_domain_cutovers SET state='authoritative',committed_at_ms=?1 \
                 WHERE domain='notes' AND state='staged'",
                    [observed_at_ms],
                )
                .map_err(|error| StorageError::sqlite("commit note cutover", error))?;
            false
        }
        _ => return Err(StorageError::ImportNotFound),
    };
    transaction
        .commit()
        .map_err(|error| StorageError::sqlite("commit note cutover", error))?;
    Ok(already)
}

fn insert_import(
    transaction: &Transaction<'_>,
    import_id: CommandId,
    source: &InspectedNoteSource,
    backup: &NoteBackupEvidence,
    revision: StateRevision,
    verified_at_ms: i64,
) -> Result<(), StorageError> {
    transaction
        .execute(
            "INSERT INTO pod0_note_imports(import_id,source_kind,source_hash,source_generation,\
         note_count,backup_byte_count,target_revision,state,verified_at_ms) \
         VALUES(?1,?2,?3,?4,?5,?6,?7,'verified',?8)",
            params![
                import_id.into_bytes().as_slice(),
                source.plan.source_kind.code(),
                source.plan.source_hash,
                i64::try_from(source.plan.source_generation).map_err(|_| {
                    StorageError::ImportLimitExceeded {
                        entity: "note source generation",
                    }
                })?,
                source.plan.note_count,
                i64::try_from(backup.byte_count).map_err(|_| {
                    StorageError::ImportLimitExceeded {
                        entity: "note backup bytes",
                    }
                })?,
                i64::try_from(revision.value).map_err(|_| StorageError::CorruptSchema {
                    detail: "note import revision is malformed",
                })?,
                verified_at_ms,
            ],
        )
        .map_err(|error| StorageError::sqlite("record note import", error))?;
    Ok(())
}

fn insert_notes(
    transaction: &Transaction<'_>,
    import_id: CommandId,
    notes: &[NoteRecord],
) -> Result<(), StorageError> {
    for note in notes {
        let (kind_code, kind_wire) = encode_kind(note.kind);
        let (author_code, author_wire) = encode_author(note.author);
        let target = encode_target(note.target)?;
        transaction
            .execute(
                "INSERT INTO pod0_notes(note_id,note_revision,text,kind_code,kind_wire_code,\
             author_code,author_wire_code,target_code,target_wire_code,target_note_id,episode_id,\
             position_ms,created_at_ms,deleted,source_import_id,created_command_id) \
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,NULL)",
                params![
                    note.note_id.into_bytes().as_slice(),
                    i64::try_from(note.revision.value).map_err(|_| {
                        StorageError::InvalidLegacyRecord {
                            entity: "note",
                            index: 0,
                            detail: "note revision is outside supported range",
                        }
                    })?,
                    note.text,
                    kind_code,
                    kind_wire,
                    author_code,
                    author_wire,
                    target.code,
                    target.wire,
                    target.note_id,
                    target.episode_id,
                    target.position_ms,
                    note.created_at.value,
                    i64::from(note.deleted),
                    import_id.into_bytes().as_slice(),
                ],
            )
            .map_err(|error| StorageError::sqlite("import note", error))?;
    }
    Ok(())
}
