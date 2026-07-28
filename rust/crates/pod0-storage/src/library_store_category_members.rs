use pod0_domain::{CategoryId, CategoryItemKind, CommandId, LibraryItemId, StateRevision};
use rusqlite::params;

use crate::StorageError;
use crate::category_store_model::encode_item_kind;
use crate::category_store_read::category_exists;
use crate::category_store_write_support::{bump_category, finish_category_command};
use crate::library_store::{LibraryStore, command_was_applied};

impl LibraryStore {
    /// Adds and removes members in one command. `resolve` maps a
    /// `LibraryItemId` to what it actually is; ids it cannot resolve are
    /// rejected rather than dropped, so a caller never believes an item was
    /// filed when it was not.
    #[allow(clippy::too_many_arguments)]
    pub fn tag_category_items(
        &self,
        command_id: CommandId,
        command_fingerprint: &str,
        category_id: CategoryId,
        add: &[LibraryItemId],
        remove: &[LibraryItemId],
        resolve: impl Fn(LibraryItemId) -> Option<CategoryItemKind>,
        observed_at_ms: i64,
    ) -> Result<(StateRevision, usize, usize), StorageError> {
        self.write(|transaction| {
            if let Some(revision) =
                command_was_applied(transaction, command_id, command_fingerprint)?
            {
                return Ok((revision, 0, 0));
            }
            if !category_exists(transaction, category_id)? {
                return Err(StorageError::EntityNotFound);
            }
            let mut added = 0;
            for item in add {
                let kind = resolve(*item).ok_or(StorageError::EntityNotFound)?;
                let changed = transaction
                    .execute(
                        "INSERT INTO pod0_category_members(category_id,item_id,item_kind_code,\
                         added_at_ms) VALUES(?1,?2,?3,?4) \
                         ON CONFLICT(category_id,item_id) DO NOTHING",
                        params![
                            category_id.into_bytes().as_slice(),
                            item.into_bytes().as_slice(),
                            encode_item_kind(kind)?,
                            observed_at_ms,
                        ],
                    )
                    .map_err(|error| StorageError::sqlite("add category member", error))?;
                added += changed;
            }
            let mut removed = 0;
            for item in remove {
                let changed = transaction
                    .execute(
                        "DELETE FROM pod0_category_members WHERE category_id=?1 AND item_id=?2",
                        params![
                            category_id.into_bytes().as_slice(),
                            item.into_bytes().as_slice(),
                        ],
                    )
                    .map_err(|error| StorageError::sqlite("remove category member", error))?;
                removed += changed;
            }
            bump_category(transaction, category_id, observed_at_ms)?;
            let revision = finish_category_command(
                transaction,
                command_id,
                command_fingerprint,
                observed_at_ms,
            )?;
            Ok((revision, added, removed))
        })
    }

    /// Every category an item currently belongs to. Backs "which lenses does
    /// this episode appear under" without loading the whole taxonomy.
    pub fn categories_for_item(
        &self,
        item_id: LibraryItemId,
    ) -> Result<Vec<CategoryId>, StorageError> {
        self.write(|transaction| {
            let mut statement = transaction
                .prepare(
                    "SELECT m.category_id FROM pod0_category_members m \
                     JOIN pod0_categories c ON c.category_id=m.category_id \
                     WHERE m.item_id=?1 AND c.deleted=0 ORDER BY c.name",
                )
                .map_err(|error| StorageError::sqlite("prepare item categories", error))?;
            let rows = statement
                .query_map([item_id.into_bytes().as_slice()], |row| {
                    row.get::<_, Vec<u8>>(0)
                })
                .map_err(|error| StorageError::sqlite("read item categories", error))?;
            let mut ids = Vec::new();
            for row in rows {
                let bytes =
                    row.map_err(|error| StorageError::sqlite("decode item category", error))?;
                let bytes = <[u8; 16]>::try_from(bytes.as_slice()).map_err(|_| {
                    StorageError::CorruptSchema {
                        detail: "category identifier is not sixteen bytes",
                    }
                })?;
                ids.push(CategoryId::from_bytes(bytes));
            }
            Ok(ids)
        })
    }
}
