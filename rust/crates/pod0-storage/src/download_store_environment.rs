use pod0_domain::{CommandId, StateRevision};
use rusqlite::{Transaction, params};

use crate::download_store_request::u64_to_i64;
use crate::library_store::finish_command;
use crate::{LibraryStore, StorageError, StoredDownloadNetwork};

impl LibraryStore {
    pub fn observe_download_environment(
        &self,
        command_id: CommandId,
        command_fingerprint: &str,
        network: StoredDownloadNetwork,
        available_capacity_bytes: Option<u64>,
        observed_at_ms: i64,
    ) -> Result<StateRevision, StorageError> {
        crate::transition_commit::commit_download_environment(
            self.path(),
            command_id,
            command_fingerprint,
            network,
            available_capacity_bytes,
            observed_at_ms,
        )
    }
}

pub(crate) fn apply_download_environment(
    transaction: &Transaction<'_>,
    command_id: CommandId,
    command_fingerprint: &str,
    network: StoredDownloadNetwork,
    available_capacity_bytes: Option<u64>,
    observed_at_ms: i64,
) -> Result<StateRevision, StorageError> {
    let (code, wire) = network.wire();
    transaction
        .execute(
            "UPDATE pod0_download_environment SET network_code=?1,network_wire_code=?2,\
             available_capacity_bytes=?3,observed_at_ms=?4 WHERE singleton=1",
            params![
                code,
                wire,
                available_capacity_bytes.map(u64_to_i64).transpose()?,
                observed_at_ms
            ],
        )
        .map_err(|error| StorageError::sqlite("update download environment", error))?;
    finish_command(transaction, command_id, command_fingerprint, observed_at_ms)
}
