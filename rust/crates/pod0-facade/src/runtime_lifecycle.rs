use pod0_recall_index::RecallIndexError;

use crate::{FacadeOpenError, SchemaBlockReason};

impl From<pod0_storage::StorageError> for FacadeOpenError {
    fn from(value: pod0_storage::StorageError) -> Self {
        match value {
            pod0_storage::StorageError::CutoverNotAuthoritative
            | pod0_storage::StorageError::ImportNotFound => Self::NotAuthoritative,
            // The store is ahead of this build. Downgrade is refused, so no
            // relaunch and no retry changes this — only a newer build does.
            pod0_storage::StorageError::NewerSchema { .. }
            | pod0_storage::StorageError::DowngradeForbidden { .. }
            | pod0_storage::StorageError::UnsupportedTarget { .. } => Self::SchemaBlocked {
                reason: SchemaBlockReason::StoreNewerThanApp,
            },
            pod0_storage::StorageError::FailedMigration { .. } => Self::SchemaBlocked {
                reason: SchemaBlockReason::MigrationFailed,
            },
            pod0_storage::StorageError::ForeignDatabase
            | pod0_storage::StorageError::CorruptSchema { .. } => Self::SchemaBlocked {
                reason: SchemaBlockReason::StoreUnreadable,
            },
            _ => Self::StorageUnavailable,
        }
    }
}

impl From<RecallIndexError> for FacadeOpenError {
    fn from(value: RecallIndexError) -> Self {
        match value {
            RecallIndexError::IncompatibleSchema => Self::SchemaBlocked {
                reason: SchemaBlockReason::StoreUnreadable,
            },
            _ => Self::StorageUnavailable,
        }
    }
}

#[cfg(test)]
mod schema_block_reason_tests {
    use super::*;
    use pod0_storage::StorageError;

    fn reason(error: StorageError) -> Option<SchemaBlockReason> {
        match FacadeOpenError::from(error) {
            FacadeOpenError::SchemaBlocked { reason } => Some(reason),
            _ => None,
        }
    }

    /// Every blocked reason must be distinguishable at the boundary. Collapsing
    /// them leaves the host with one recovery instruction for three different
    /// remedies, which is how the recovery screen came to tell users to relaunch
    /// in the one case where relaunching can never work.
    #[test]
    fn blocked_storage_errors_map_to_the_remedy_not_the_cause() {
        assert_eq!(
            reason(StorageError::NewerSchema {
                stored: 33,
                supported: 32
            }),
            Some(SchemaBlockReason::StoreNewerThanApp)
        );
        assert_eq!(
            reason(StorageError::DowngradeForbidden {
                stored: 33,
                requested: 32
            }),
            Some(SchemaBlockReason::StoreNewerThanApp)
        );
        assert_eq!(
            reason(StorageError::UnsupportedTarget {
                requested: 99,
                supported: 33
            }),
            Some(SchemaBlockReason::StoreNewerThanApp)
        );
        assert_eq!(
            reason(StorageError::FailedMigration { from: 32, to: 33 }),
            Some(SchemaBlockReason::MigrationFailed)
        );
        assert_eq!(
            reason(StorageError::ForeignDatabase),
            Some(SchemaBlockReason::StoreUnreadable)
        );
        assert_eq!(
            reason(StorageError::CorruptSchema { detail: "notes" }),
            Some(SchemaBlockReason::StoreUnreadable)
        );
    }

    /// Non-schema failures must not acquire a schema remedy.
    #[test]
    fn unrelated_storage_errors_are_not_schema_blocked() {
        assert_eq!(reason(StorageError::EntityNotFound), None);
        assert!(matches!(
            FacadeOpenError::from(StorageError::CutoverNotAuthoritative),
            FacadeOpenError::NotAuthoritative
        ));
    }
}
