use pod0_domain::{
    AutoDownloadPolicy, CategoryItemKind, CategoryOrigin, CategoryRecord, CategorySettings,
    StateRevision,
};

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

pub(crate) fn encode_settings(
    settings: CategorySettings,
) -> Result<(Option<i64>, Option<i64>, Option<i64>, i64, i64), StorageError> {
    let (code, latest, wifi_only) = match settings.auto_download_override {
        None => (None, None, None),
        Some(policy) => {
            pod0_domain::validate_category_settings(settings)
                .map_err(|_| StorageError::InvalidCategory)?;
            let (code, wire, latest) = crate::listening_db_codec::auto_download(&policy.mode);
            if wire.is_some() {
                return Err(StorageError::InvalidCategory);
            }
            (Some(code), latest, Some(i64::from(policy.wifi_only)))
        }
    };
    Ok((
        code,
        latest,
        wifi_only,
        i64::from(settings.rag_enabled),
        i64::from(settings.notifications_enabled),
    ))
}

pub(crate) fn decode_settings(
    code: Option<i64>,
    latest: Option<i64>,
    wifi_only: Option<i64>,
    rag_enabled: i64,
    notifications_enabled: i64,
) -> Result<CategorySettings, StorageError> {
    let auto_download_override = match (code, wifi_only) {
        (None, None) if latest.is_none() => None,
        (Some(code), Some(wifi_only)) => Some(AutoDownloadPolicy {
            mode: crate::listening_db_codec::decode_auto_download(code, None, latest)?,
            wifi_only: boolean(wifi_only)?,
        }),
        _ => return Err(corrupt("category auto-download override is malformed")),
    };
    Ok(CategorySettings {
        auto_download_override,
        rag_enabled: boolean(rag_enabled)?,
        notifications_enabled: boolean(notifications_enabled)?,
    })
}

fn boolean(value: i64) -> Result<bool, StorageError> {
    match value {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(corrupt("category boolean is malformed")),
    }
}

const fn corrupt(detail: &'static str) -> StorageError {
    StorageError::CorruptSchema { detail }
}
