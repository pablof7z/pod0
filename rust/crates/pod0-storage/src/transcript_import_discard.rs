use std::path::Path;

use pod0_domain::CommandId;
use rusqlite::{TransactionBehavior, params};

use crate::StorageError;
use crate::migration_db::configure;
use crate::transcript_import_model::{TranscriptImportReport, TranscriptImportState};
use crate::transcript_import_store_read::{open_current, read_import_report};

pub(crate) fn discard_transcript_import(
    target_path: &Path,
    import_id: CommandId,
    discarded_at_ms: i64,
) -> Result<TranscriptImportReport, StorageError> {
    discard_transcript_import_with_diagnostic(
        target_path,
        import_id,
        discarded_at_ms,
        "discarded_by_caller",
    )
}

pub(crate) fn discard_transcript_import_with_diagnostic(
    target_path: &Path,
    import_id: CommandId,
    discarded_at_ms: i64,
    diagnostic: &'static str,
) -> Result<TranscriptImportReport, StorageError> {
    if discarded_at_ms < 0 {
        return Err(StorageError::TranscriptImportConflict);
    }
    let mut connection = open_current(target_path)?;
    configure(&connection)?;
    let current = read_import_report(&connection, import_id, true)?
        .ok_or(StorageError::TranscriptImportNotFound)?;
    if current.state == TranscriptImportState::Committed {
        return Err(StorageError::TranscriptImportConflict);
    }
    if current.state != TranscriptImportState::Discarded {
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| StorageError::sqlite("begin transcript import discard", error))?;
        transaction
            .execute(
                "UPDATE pod0_transcript_imports SET state='discarded',diagnostic_code=?1,\
                 verified_at_ms=NULL,committed_at_ms=NULL,discarded_at_ms=?2 WHERE import_id=?3",
                params![
                    diagnostic,
                    discarded_at_ms,
                    import_id.into_bytes().as_slice()
                ],
            )
            .map_err(|error| StorageError::sqlite("discard transcript import", error))?;
        transaction
            .execute(
                "DELETE FROM pod0_domain_cutovers WHERE domain='transcripts' AND state='staged' \
                 AND core_revision=?1",
                [to_i64(current.target_revision.value)?],
            )
            .map_err(|error| StorageError::sqlite("discard transcript cutover marker", error))?;
        transaction
            .commit()
            .map_err(|error| StorageError::sqlite("commit transcript import discard", error))?;
    }
    read_import_report(&connection, import_id, true)?.ok_or(StorageError::TranscriptImportNotFound)
}

fn to_i64(value: u64) -> Result<i64, StorageError> {
    i64::try_from(value).map_err(|_| StorageError::TranscriptImportConflict)
}
