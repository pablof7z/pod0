use pod0_domain::{CategoryId, CategoryOrigin, CommandId, StateRevision, category_slug};
use rusqlite::params;

use crate::StorageError;
use crate::category_store_model::encode_origin;
use crate::category_store_write_support::{bump_category, finish_category_command};
use crate::library_store::LibraryStore;

/// Field-level edit intent. `None` means "leave as it is" — distinct from a
/// present value, which is why this is not just `Option<String>` threaded
/// through with a sentinel.
#[derive(Clone, Debug, Default)]
pub struct CategoryEdit {
    pub name: Option<String>,
    pub description: Option<String>,
    pub color_hex: Option<String>,
}

impl LibraryStore {
    /// Creates a category. The id is derived from `command_id` so a replayed
    /// command lands on the same row rather than minting a duplicate.
    #[allow(clippy::too_many_arguments)]
    pub fn create_category(
        &self,
        command_id: CommandId,
        command_fingerprint: &str,
        name: &str,
        description: &str,
        color_hex: Option<&str>,
        origin: CategoryOrigin,
        observed_at_ms: i64,
    ) -> Result<(StateRevision, CategoryId), StorageError> {
        crate::transition_commit::commit_category_create(
            self.path(),
            command_id,
            command_fingerprint,
            name,
            description,
            color_hex,
            origin,
            observed_at_ms,
        )
    }

    pub fn update_category(
        &self,
        command_id: CommandId,
        command_fingerprint: &str,
        category_id: CategoryId,
        edit: &CategoryEdit,
        observed_at_ms: i64,
    ) -> Result<StateRevision, StorageError> {
        crate::transition_commit::commit_category_update(
            self.path(),
            command_id,
            command_fingerprint,
            category_id,
            edit,
            observed_at_ms,
        )
    }

    /// Soft-deletes a category. Membership rows go with it, but the shows and
    /// episodes themselves are untouched — a category is a lens, not a
    /// container.
    pub fn delete_category(
        &self,
        command_id: CommandId,
        command_fingerprint: &str,
        category_id: CategoryId,
        observed_at_ms: i64,
    ) -> Result<StateRevision, StorageError> {
        crate::transition_commit::commit_category_delete(
            self.path(),
            command_id,
            command_fingerprint,
            category_id,
            observed_at_ms,
        )
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn create_category_in_transaction(
    transaction: &rusqlite::Transaction<'_>,
    command_id: CommandId,
    fingerprint: &str,
    category_id: CategoryId,
    name: &str,
    description: &str,
    color_hex: Option<&str>,
    origin: CategoryOrigin,
    observed_at_ms: i64,
) -> Result<StateRevision, StorageError> {
    transaction.execute(
        "INSERT INTO pod0_categories(category_id,category_revision,name,slug,description,color_hex,origin_code,created_at_ms,updated_at_ms,deleted,created_command_id) VALUES(?1,1,?2,?3,?4,?5,?6,?7,?7,0,?8)",
        params![category_id.into_bytes().as_slice(), name, slug_or_id(name, category_id), description, color_hex, encode_origin(origin)?, observed_at_ms, command_id.into_bytes().as_slice()],
    ).map_err(|error| StorageError::sqlite("create category", error))?;
    finish_category_command(transaction, command_id, fingerprint, observed_at_ms)
}

pub(crate) fn update_category_in_transaction(
    transaction: &rusqlite::Transaction<'_>,
    command_id: CommandId,
    fingerprint: &str,
    category_id: CategoryId,
    edit: &CategoryEdit,
    observed_at_ms: i64,
) -> Result<StateRevision, StorageError> {
    if let Some(name) = &edit.name {
        transaction
            .execute(
                "UPDATE pod0_categories SET name=?2,slug=?3 WHERE category_id=?1",
                params![
                    category_id.into_bytes().as_slice(),
                    name,
                    slug_or_id(name, category_id)
                ],
            )
            .map_err(|error| StorageError::sqlite("rename category", error))?;
    }
    if let Some(description) = &edit.description {
        transaction
            .execute(
                "UPDATE pod0_categories SET description=?2 WHERE category_id=?1",
                params![category_id.into_bytes().as_slice(), description],
            )
            .map_err(|error| StorageError::sqlite("describe category", error))?;
    }
    if let Some(color_hex) = &edit.color_hex {
        transaction
            .execute(
                "UPDATE pod0_categories SET color_hex=?2 WHERE category_id=?1",
                params![category_id.into_bytes().as_slice(), color_hex],
            )
            .map_err(|error| StorageError::sqlite("tint category", error))?;
    }
    bump_category(transaction, category_id, observed_at_ms)?;
    finish_category_command(transaction, command_id, fingerprint, observed_at_ms)
}

pub(crate) fn delete_category_in_transaction(
    transaction: &rusqlite::Transaction<'_>,
    command_id: CommandId,
    fingerprint: &str,
    category_id: CategoryId,
    observed_at_ms: i64,
) -> Result<StateRevision, StorageError> {
    transaction
        .execute(
            "DELETE FROM pod0_category_members WHERE category_id=?1",
            [category_id.into_bytes().as_slice()],
        )
        .map_err(|error| StorageError::sqlite("clear category members", error))?;
    transaction
        .execute(
            "UPDATE pod0_categories SET deleted=1,updated_at_ms=?2 WHERE category_id=?1",
            params![category_id.into_bytes().as_slice(), observed_at_ms],
        )
        .map_err(|error| StorageError::sqlite("delete category", error))?;
    finish_category_command(transaction, command_id, fingerprint, observed_at_ms)
}

/// A name with no ASCII characters slugs to nothing, which would make every
/// such category share one routing key. Fall back to the id instead.
pub(crate) fn slug_or_id(name: &str, category_id: CategoryId) -> String {
    let slug = category_slug(name);
    if slug.is_empty() {
        format!("c-{:016x}", category_id.high())
    } else {
        slug
    }
}
