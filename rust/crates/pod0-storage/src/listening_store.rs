use std::path::Path;

use pod0_domain::CommandId;

use crate::import_model::ListeningImportVerification;
use crate::listening_store_read::{read_snapshot, stored_import_report};
use crate::migration_db::{open_connection, user_version, validate_open_database};
use crate::{CURRENT_SCHEMA_VERSION, StorageError};

pub fn read_listening_import(
    path: &Path,
    import_id: CommandId,
) -> Result<ListeningImportVerification, StorageError> {
    let connection = open_connection(path, true)?;
    let version = user_version(&connection)?;
    validate_open_database(&connection, version)?;
    if version != CURRENT_SCHEMA_VERSION {
        return Err(StorageError::CorruptSchema {
            detail: "listening import store is not current",
        });
    }
    let report =
        stored_import_report(&connection, import_id, None)?.ok_or(StorageError::ImportNotFound)?;
    if !report.staged {
        return Err(StorageError::CorruptSchema {
            detail: "listening import is not staged",
        });
    }
    let snapshot = read_snapshot(&connection)?;
    if snapshot.podcasts.len() != report.plan.podcast_count as usize
        || snapshot.subscriptions.len() != report.plan.subscription_count as usize
        || snapshot.episodes.len() != report.plan.episode_count as usize
        || snapshot.playback.revision.value != report.target_revision
    {
        return Err(StorageError::CorruptSchema {
            detail: "listening import counts or revision differ",
        });
    }
    Ok(ListeningImportVerification { report, snapshot })
}
