use std::collections::BTreeSet;

use pod0_domain::{
    CategoryId, CategoryItemKind, CategoryReplacementInput, CommandId, LibraryItemId,
    MAX_CATEGORIES, StateRevision, validate_category, validate_category_settings,
};
use rusqlite::{OptionalExtension, Transaction, params};

use crate::category_store_model::{encode_item_kind, encode_origin, encode_settings};
use crate::category_store_write_support::{bump_category, finish_category_command};
use crate::{LibraryStore, StorageError};

impl LibraryStore {
    pub fn category_store_is_authoritative(&self) -> Result<bool, StorageError> {
        self.read(category_store_is_authoritative)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn import_legacy_categories(
        &self,
        command_id: CommandId,
        fingerprint: &str,
        source_generation: u64,
        categories: &[CategoryReplacementInput],
        observed_at_ms: i64,
    ) -> Result<StateRevision, StorageError> {
        self.write(|transaction| {
            if category_store_is_authoritative(transaction)? {
                return crate::category_store_read::collection_revision(transaction);
            }
            validate_replacement(transaction, categories)?;
            replace_rows(transaction, command_id, categories, observed_at_ms)?;
            let revision = finish_category_command(
                transaction,
                command_id,
                fingerprint,
                observed_at_ms,
            )?;
            transaction.execute(
                "INSERT INTO pod0_domain_cutovers(domain,state,source_generation,core_revision,committed_at_ms) \
                 VALUES('categories','authoritative',?1,?2,?3)",
                params![stored_u64(source_generation)?, stored_u64(revision.value)?, observed_at_ms],
            ).map_err(|error| StorageError::sqlite("commit category authority", error))?;
            Ok(revision)
        })
    }

    pub fn replace_categories(
        &self,
        command_id: CommandId,
        fingerprint: &str,
        expected_revision: StateRevision,
        categories: &[CategoryReplacementInput],
        observed_at_ms: i64,
    ) -> Result<StateRevision, StorageError> {
        self.write(|transaction| {
            require_category_authority(transaction)?;
            if let Some(revision) =
                crate::library_store::command_was_applied(transaction, command_id, fingerprint)?
            {
                return Ok(revision);
            }
            require_revision(transaction, expected_revision)?;
            validate_replacement(transaction, categories)?;
            replace_rows(transaction, command_id, categories, observed_at_ms)?;
            finish_category_command(transaction, command_id, fingerprint, observed_at_ms)
        })
    }

    pub fn move_podcast_to_category(
        &self,
        command_id: CommandId,
        fingerprint: &str,
        expected_revision: StateRevision,
        podcast_id: pod0_domain::PodcastId,
        category_id: CategoryId,
        observed_at_ms: i64,
    ) -> Result<StateRevision, StorageError> {
        self.write(|transaction| {
            require_category_authority(transaction)?;
            if let Some(revision) = crate::library_store::command_was_applied(
                transaction,
                command_id,
                fingerprint,
            )? {
                return Ok(revision);
            }
            require_revision(transaction, expected_revision)?;
            if !crate::category_store_read::category_exists(transaction, category_id)?
                || !podcast_is_subscribed(transaction, podcast_id)?
            {
                return Err(StorageError::EntityNotFound);
            }
            let item_id = LibraryItemId::from_bytes(podcast_id.into_bytes());
            let mut affected = categories_for_item(transaction, item_id)?;
            if !affected.contains(&category_id) {
                affected.push(category_id);
            }
            transaction.execute(
                "DELETE FROM pod0_category_members WHERE item_id=?1 AND item_kind_code=1",
                [item_id.into_bytes().as_slice()],
            ).map_err(|error| StorageError::sqlite("move podcast out of categories", error))?;
            transaction.execute(
                "INSERT INTO pod0_category_members(category_id,item_id,item_kind_code,added_at_ms) \
                 VALUES(?1,?2,1,?3)",
                params![category_id.into_bytes().as_slice(), item_id.into_bytes().as_slice(), observed_at_ms],
            ).map_err(|error| StorageError::sqlite("move podcast into category", error))?;
            for affected_id in affected {
                bump_category(transaction, affected_id, observed_at_ms)?;
            }
            finish_category_command(transaction, command_id, fingerprint, observed_at_ms)
        })
    }
}

pub(crate) fn category_store_is_authoritative(
    connection: &rusqlite::Connection,
) -> Result<bool, StorageError> {
    connection
        .query_row(
            "SELECT state='authoritative' FROM pod0_domain_cutovers WHERE domain='categories'",
            [],
            |row| row.get(0),
        )
        .optional()
        .map(|value| value.unwrap_or(false))
        .map_err(|error| StorageError::sqlite("read category authority", error))
}

pub(crate) fn require_category_authority(
    connection: &rusqlite::Connection,
) -> Result<(), StorageError> {
    category_store_is_authoritative(connection)?
        .then_some(())
        .ok_or(StorageError::CutoverNotAuthoritative)
}

fn validate_replacement(
    transaction: &Transaction<'_>,
    categories: &[CategoryReplacementInput],
) -> Result<(), StorageError> {
    if categories.len() > MAX_CATEGORIES {
        return Err(StorageError::InvalidCategory);
    }
    let mut category_ids = BTreeSet::new();
    let mut podcast_ids = BTreeSet::new();
    for category in categories {
        if !category_ids.insert(category.category_id)
            || validate_category(
                &category.name,
                &category.description,
                category.color_hex.as_deref(),
                category.origin,
            )
            .is_err()
            || validate_category_settings(category.settings).is_err()
        {
            return Err(StorageError::InvalidCategory);
        }
        for podcast_id in &category.podcast_ids {
            if !podcast_ids.insert(*podcast_id) || !podcast_is_subscribed(transaction, *podcast_id)?
            {
                return Err(StorageError::InvalidCategory);
            }
        }
    }
    Ok(())
}

fn replace_rows(
    transaction: &Transaction<'_>,
    command_id: CommandId,
    categories: &[CategoryReplacementInput],
    observed_at_ms: i64,
) -> Result<(), StorageError> {
    transaction
        .execute("DELETE FROM pod0_category_members", [])
        .map_err(|error| StorageError::sqlite("replace category members", error))?;
    transaction
        .execute("DELETE FROM pod0_category_settings", [])
        .map_err(|error| StorageError::sqlite("replace category settings", error))?;
    transaction
        .execute("DELETE FROM pod0_categories", [])
        .map_err(|error| StorageError::sqlite("replace categories", error))?;
    for category in categories {
        insert_category(transaction, command_id, category, observed_at_ms)?;
    }
    Ok(())
}

fn insert_category(
    transaction: &Transaction<'_>,
    command_id: CommandId,
    category: &CategoryReplacementInput,
    observed_at_ms: i64,
) -> Result<(), StorageError> {
    let created = category.generated_at.value;
    transaction.execute(
        "INSERT INTO pod0_categories(category_id,category_revision,name,slug,description,color_hex,\
         origin_code,created_at_ms,updated_at_ms,deleted,created_command_id) \
         VALUES(?1,1,?2,?3,?4,?5,?6,?7,?8,0,?9)",
        params![category.category_id.into_bytes().as_slice(), category.name,
            crate::library_store_categories::slug_or_id(&category.name, category.category_id),
            category.description, category.color_hex, encode_origin(category.origin)?, created,
            observed_at_ms, command_id.into_bytes().as_slice()],
    ).map_err(|error| StorageError::sqlite("insert replacement category", error))?;
    let (code, latest, wifi, rag, notifications) = encode_settings(category.settings)?;
    transaction
        .execute(
            "INSERT INTO pod0_category_settings(category_id,auto_download_code,\
         auto_download_latest_count,wifi_only,rag_enabled,notifications_enabled,updated_at_ms) \
         VALUES(?1,?2,?3,?4,?5,?6,?7)",
            params![
                category.category_id.into_bytes().as_slice(),
                code,
                latest,
                wifi,
                rag,
                notifications,
                observed_at_ms
            ],
        )
        .map_err(|error| StorageError::sqlite("insert replacement category settings", error))?;
    for podcast_id in &category.podcast_ids {
        let item_id = LibraryItemId::from_bytes(podcast_id.into_bytes());
        transaction
            .execute(
                "INSERT INTO pod0_category_members(category_id,item_id,item_kind_code,added_at_ms) \
             VALUES(?1,?2,?3,?4)",
                params![
                    category.category_id.into_bytes().as_slice(),
                    item_id.into_bytes().as_slice(),
                    encode_item_kind(CategoryItemKind::Podcast)?,
                    observed_at_ms
                ],
            )
            .map_err(|error| StorageError::sqlite("insert replacement category member", error))?;
    }
    Ok(())
}

fn podcast_is_subscribed(
    connection: &rusqlite::Connection,
    podcast_id: pod0_domain::PodcastId,
) -> Result<bool, StorageError> {
    let count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM pod0_subscriptions WHERE podcast_id=?1",
            [podcast_id.into_bytes().as_slice()],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("validate category podcast", error))?;
    Ok(count == 1)
}

fn categories_for_item(
    connection: &rusqlite::Connection,
    item_id: LibraryItemId,
) -> Result<Vec<CategoryId>, StorageError> {
    let mut statement = connection
        .prepare(
            "SELECT category_id FROM pod0_category_members WHERE item_id=?1 AND item_kind_code=1",
        )
        .map_err(|error| StorageError::sqlite("read moved podcast categories", error))?;
    let rows = statement
        .query_map([item_id.into_bytes().as_slice()], |row| {
            row.get::<_, Vec<u8>>(0)
        })
        .map_err(|error| StorageError::sqlite("read moved podcast categories", error))?;
    let mut ids = Vec::new();
    for row in rows {
        let bytes =
            row.map_err(|error| StorageError::sqlite("decode moved podcast category", error))?;
        ids.push(CategoryId::from_bytes(bytes.try_into().map_err(|_| {
            StorageError::CorruptSchema {
                detail: "category identifier width",
            }
        })?));
    }
    Ok(ids)
}

fn require_revision(
    connection: &rusqlite::Connection,
    expected: StateRevision,
) -> Result<(), StorageError> {
    (crate::category_store_read::collection_revision(connection)? == expected)
        .then_some(())
        .ok_or(StorageError::RevisionConflict)
}

fn stored_u64(value: u64) -> Result<i64, StorageError> {
    i64::try_from(value).map_err(|_| StorageError::InvalidCategory)
}
