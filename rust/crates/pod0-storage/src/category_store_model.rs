use pod0_domain::{CategoryItemKind, CategoryOrigin, CategoryRecord, StateRevision};

use crate::StorageError;

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct CategoryCollectionSnapshot {
    pub revision: StateRevision,
    /// Active categories only, ordered by name. Deleted rows are retained in
    /// storage for command replay but never surface in a snapshot.
    pub categories: Vec<CategoryRecord>,
}

pub(crate) fn encode_origin(origin: CategoryOrigin) -> Result<i64, StorageError> {
    match origin {
        CategoryOrigin::Generated => Ok(1),
        CategoryOrigin::Agent => Ok(2),
        CategoryOrigin::User => Ok(3),
        CategoryOrigin::Unsupported { .. } => Err(StorageError::InvalidCategory),
    }
}

pub(crate) fn decode_origin(code: i64) -> Result<CategoryOrigin, StorageError> {
    match code {
        1 => Ok(CategoryOrigin::Generated),
        2 => Ok(CategoryOrigin::Agent),
        3 => Ok(CategoryOrigin::User),
        // A row the schema CHECK should have rejected means the file was
        // written by something other than this kernel.
        _ => Err(StorageError::CorruptSchema {
            detail: "category origin code is unsupported",
        }),
    }
}

pub(crate) fn encode_item_kind(kind: CategoryItemKind) -> Result<i64, StorageError> {
    match kind {
        CategoryItemKind::Podcast => Ok(1),
        CategoryItemKind::Episode => Ok(2),
        CategoryItemKind::Unsupported { .. } => Err(StorageError::InvalidCategory),
    }
}

pub(crate) fn decode_item_kind(code: i64) -> Result<CategoryItemKind, StorageError> {
    match code {
        1 => Ok(CategoryItemKind::Podcast),
        2 => Ok(CategoryItemKind::Episode),
        _ => Err(StorageError::CorruptSchema {
            detail: "category item kind code is unsupported",
        }),
    }
}
