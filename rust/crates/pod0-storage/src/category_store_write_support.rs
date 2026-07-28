use pod0_domain::{CategoryId, CommandId, StateRevision};
use rusqlite::{OptionalExtension, Transaction, params};

use crate::StorageError;
use crate::category_store_read::collection_revision;
use crate::library_store::finish_command;

pub(crate) fn bump_category(
    transaction: &Transaction<'_>,
    category_id: CategoryId,
    observed_at_ms: i64,
) -> Result<(), StorageError> {
    let current: Option<i64> = transaction
        .query_row(
            "SELECT category_revision FROM pod0_categories WHERE category_id=?1",
            [category_id.into_bytes().as_slice()],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read category revision", error))?;
    let current = current.ok_or(StorageError::EntityNotFound)?;
    transaction
        .execute(
            "UPDATE pod0_categories SET category_revision=?2,updated_at_ms=?3 \
             WHERE category_id=?1",
            params![
                category_id.into_bytes().as_slice(),
                current.saturating_add(1),
                observed_at_ms,
            ],
        )
        .map_err(|error| StorageError::sqlite("advance category revision", error))?;
    Ok(())
}

pub(crate) fn finish_category_command(
    transaction: &Transaction<'_>,
    command_id: CommandId,
    fingerprint: &str,
    observed_at_ms: i64,
) -> Result<StateRevision, StorageError> {
    let revision = finish_command(transaction, command_id, fingerprint, observed_at_ms)?;
    let value = i64::try_from(revision.value).map_err(|_| StorageError::CorruptSchema {
        detail: "category collection revision is malformed",
    })?;
    transaction
        .execute(
            "UPDATE pod0_category_state SET collection_revision=?1 WHERE singleton=1",
            [value],
        )
        .map_err(|error| StorageError::sqlite("advance category collection revision", error))?;
    debug_assert!(collection_revision(transaction).is_ok());
    Ok(revision)
}
