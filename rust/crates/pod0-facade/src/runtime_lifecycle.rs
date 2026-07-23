use pod0_recall_index::RecallIndexError;

use crate::{FacadeOpenError, Pod0Facade};

impl Drop for Pod0Facade {
    fn drop(&mut self) {
        let runtime = self
            .nmp
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        if let Some(runtime) = runtime {
            runtime.shutdown();
            drop(runtime);
        }
        if let Some(dispatcher) = self
            .nmp_dispatcher
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
        {
            let _ = dispatcher.join();
        }
    }
}

impl From<pod0_storage::StorageError> for FacadeOpenError {
    fn from(value: pod0_storage::StorageError) -> Self {
        match value {
            pod0_storage::StorageError::CutoverNotAuthoritative
            | pod0_storage::StorageError::ImportNotFound => Self::NotAuthoritative,
            pod0_storage::StorageError::ForeignDatabase
            | pod0_storage::StorageError::CorruptSchema { .. }
            | pod0_storage::StorageError::NewerSchema { .. }
            | pod0_storage::StorageError::FailedMigration { .. }
            | pod0_storage::StorageError::DowngradeForbidden { .. }
            | pod0_storage::StorageError::UnsupportedTarget { .. } => Self::SchemaBlocked,
            _ => Self::StorageUnavailable,
        }
    }
}

impl From<RecallIndexError> for FacadeOpenError {
    fn from(value: RecallIndexError) -> Self {
        match value {
            RecallIndexError::IncompatibleSchema => Self::SchemaBlocked,
            _ => Self::StorageUnavailable,
        }
    }
}
