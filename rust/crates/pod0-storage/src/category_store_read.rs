use pod0_domain::{
    CategoryId, CategoryMember, CategoryRecord, CategoryRevision, LibraryItemId, StateRevision,
    UnixTimestampMilliseconds,
};
use rusqlite::{Connection, OptionalExtension};

use crate::StorageError;
use crate::category_store_model::{
    CategoryCollectionSnapshot, decode_item_kind, decode_origin, decode_settings,
};

pub(crate) fn collection_revision(connection: &Connection) -> Result<StateRevision, StorageError> {
    let value: i64 = connection
        .query_row(
            "SELECT collection_revision FROM pod0_category_state WHERE singleton=1",
            [],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("read category collection revision", error))?;
    Ok(StateRevision::new(u64::try_from(value).map_err(|_| {
        StorageError::CorruptSchema {
            detail: "category collection revision is malformed",
        }
    })?))
}

pub(crate) fn read_snapshot(
    connection: &Connection,
) -> Result<CategoryCollectionSnapshot, StorageError> {
    let revision = collection_revision(connection)?;
    let mut statement = connection
        .prepare(
            "SELECT c.category_id,c.category_revision,c.name,c.slug,c.description,c.color_hex,\
             c.origin_code,c.created_at_ms,c.updated_at_ms,s.auto_download_code,\
             s.auto_download_latest_count,s.wifi_only,s.rag_enabled,s.notifications_enabled \
             FROM pod0_categories c JOIN pod0_category_settings s USING(category_id) \
             WHERE c.deleted=0 ORDER BY c.name,c.category_id",
        )
        .map_err(|error| StorageError::sqlite("prepare category read", error))?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, Vec<u8>>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, i64>(6)?,
                row.get::<_, i64>(7)?,
                row.get::<_, i64>(8)?,
                row.get::<_, Option<i64>>(9)?,
                row.get::<_, Option<i64>>(10)?,
                row.get::<_, Option<i64>>(11)?,
                row.get::<_, i64>(12)?,
                row.get::<_, i64>(13)?,
            ))
        })
        .map_err(|error| StorageError::sqlite("read categories", error))?;

    let mut categories = Vec::new();
    for row in rows {
        let (
            id,
            revision_value,
            name,
            slug,
            description,
            color_hex,
            origin,
            created,
            updated,
            auto_download_code,
            auto_download_latest_count,
            wifi_only,
            rag_enabled,
            notifications_enabled,
        ) = row.map_err(|error| StorageError::sqlite("decode category row", error))?;
        let category_id = CategoryId::from_bytes(exactly_sixteen(&id)?);
        categories.push(CategoryRecord {
            category_id,
            revision: CategoryRevision::new(u64::try_from(revision_value).map_err(|_| {
                StorageError::CorruptSchema {
                    detail: "category revision is malformed",
                }
            })?),
            name,
            slug,
            description,
            color_hex,
            origin: decode_origin(origin)?,
            settings: decode_settings(
                auto_download_code,
                auto_download_latest_count,
                wifi_only,
                rag_enabled,
                notifications_enabled,
            )?,
            members: Vec::new(),
            created_at: UnixTimestampMilliseconds::new(created),
            updated_at: UnixTimestampMilliseconds::new(updated),
            deleted: false,
        });
    }

    for category in &mut categories {
        category.members = read_members(connection, category.category_id)?;
    }
    Ok(CategoryCollectionSnapshot {
        revision,
        categories,
    })
}

fn read_members(
    connection: &Connection,
    category_id: CategoryId,
) -> Result<Vec<CategoryMember>, StorageError> {
    let mut statement = connection
        .prepare(
            "SELECT item_id,item_kind_code,added_at_ms FROM pod0_category_members \
             WHERE category_id=?1 ORDER BY added_at_ms DESC,item_id",
        )
        .map_err(|error| StorageError::sqlite("prepare category member read", error))?;
    let rows = statement
        .query_map([category_id.into_bytes().as_slice()], |row| {
            Ok((
                row.get::<_, Vec<u8>>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })
        .map_err(|error| StorageError::sqlite("read category members", error))?;
    let mut members = Vec::new();
    for row in rows {
        let (item, kind, added) =
            row.map_err(|error| StorageError::sqlite("decode category member", error))?;
        members.push(CategoryMember {
            item_id: LibraryItemId::from_bytes(exactly_sixteen(&item)?),
            kind: decode_item_kind(kind)?,
            added_at: UnixTimestampMilliseconds::new(added),
        });
    }
    Ok(members)
}

pub(crate) fn category_exists(
    connection: &Connection,
    category_id: CategoryId,
) -> Result<bool, StorageError> {
    let count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM pod0_categories WHERE category_id=?1 AND deleted=0",
            [category_id.into_bytes().as_slice()],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("check category exists", error))?;
    Ok(count > 0)
}

pub(crate) fn category_revision(
    connection: &Connection,
    category_id: CategoryId,
) -> Result<Option<CategoryRevision>, StorageError> {
    let value = connection
        .query_row(
            "SELECT category_revision FROM pod0_categories \
             WHERE category_id=?1 AND deleted=0",
            [category_id.into_bytes().as_slice()],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read category revision", error))?;
    value
        .map(|revision| {
            u64::try_from(revision)
                .map(CategoryRevision::new)
                .map_err(|_| StorageError::CorruptSchema {
                    detail: "category revision is malformed",
                })
        })
        .transpose()
}

pub(crate) fn active_category_count(connection: &Connection) -> Result<usize, StorageError> {
    let count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM pod0_categories WHERE deleted=0",
            [],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("count categories", error))?;
    usize::try_from(count).map_err(|_| StorageError::CorruptSchema {
        detail: "category count is malformed",
    })
}

fn exactly_sixteen(value: &[u8]) -> Result<[u8; 16], StorageError> {
    <[u8; 16]>::try_from(value).map_err(|_| StorageError::CorruptSchema {
        detail: "category identifier is not sixteen bytes",
    })
}
