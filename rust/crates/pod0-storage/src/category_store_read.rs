use pod0_domain::{
    CategoryId, CategoryMember, CategoryRecord, CategoryRevision, LibraryItemId, StateRevision,
    UnixTimestampMilliseconds,
};
use rusqlite::Connection;

use crate::StorageError;
use crate::category_store_model::{CategoryCollectionSnapshot, decode_item_kind, decode_origin};

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
            "SELECT category_id,category_revision,name,slug,description,color_hex,\
             origin_code,created_at_ms,updated_at_ms \
             FROM pod0_categories WHERE deleted=0 ORDER BY name,category_id",
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
            ))
        })
        .map_err(|error| StorageError::sqlite("read categories", error))?;

    let mut categories = Vec::new();
    for row in rows {
        let (id, revision_value, name, slug, description, color_hex, origin, created, updated) =
            row.map_err(|error| StorageError::sqlite("decode category row", error))?;
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
