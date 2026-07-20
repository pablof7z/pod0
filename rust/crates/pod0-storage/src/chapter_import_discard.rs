use std::path::Path;

use pod0_domain::CommandId;
use rusqlite::{TransactionBehavior, params};

use crate::chapter_import_store_read::{open_current, read_import_report};
use crate::{ChapterImportReport, ChapterImportState, StorageError};

pub(crate) fn discard_chapter_import(
    target_path: &Path,
    import_id: CommandId,
    discarded_at_ms: i64,
) -> Result<ChapterImportReport, StorageError> {
    if discarded_at_ms < 0 {
        return Err(StorageError::ChapterImportConflict);
    }
    let mut connection = open_current(target_path)?;
    let report = read_import_report(&connection, import_id, true)?
        .ok_or(StorageError::ChapterImportNotFound)?;
    if report.state == ChapterImportState::Discarded {
        return Ok(report);
    }
    if report.state == ChapterImportState::Imported {
        return Err(StorageError::ChapterImportConflict);
    }
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| StorageError::sqlite("begin chapter import discard", error))?;
    let changed = transaction
        .execute(
            "UPDATE pod0_chapter_imports SET state='discarded',discarded_at_ms=?1 \
             WHERE import_id=?2 AND state IN ('staged','verified','corrupt')",
            params![discarded_at_ms, import_id.into_bytes().as_slice()],
        )
        .map_err(|error| StorageError::sqlite("discard chapter import", error))?;
    if changed != 1 {
        return Err(StorageError::ChapterImportConflict);
    }
    transaction
        .commit()
        .map_err(|error| StorageError::sqlite("commit chapter import discard", error))?;
    read_import_report(&connection, import_id, true)?.ok_or(StorageError::ChapterImportNotFound)
}
