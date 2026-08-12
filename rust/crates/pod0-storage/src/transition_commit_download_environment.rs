use pod0_application::{
    DownloadEnvironmentActivityInput, DownloadEnvironmentMutation, plan_download_environment,
};
use pod0_domain::{CommandId, StateRevision, UnixTimestampMilliseconds};

use super::TransitionCommit;
use super::application_support::{fingerprint, legacy_library_receipt};
use crate::download_store_environment::apply_download_environment;
use crate::{StorageError, StoredDownloadNetwork, TransitionIngress, TransitionIngressKind};

pub(crate) fn commit_download_environment(
    path: &std::path::Path,
    command_id: CommandId,
    command_fingerprint: &str,
    network: StoredDownloadNetwork,
    available_capacity_bytes: Option<u64>,
    observed_at_ms: i64,
) -> Result<StateRevision, StorageError> {
    let ingress = TransitionIngress {
        kind: TransitionIngressKind::ApplicationCommand,
        id: command_id.into_bytes(),
        fingerprint: fingerprint(command_fingerprint)?,
    };
    let receipt = TransitionCommit::open(path)?.commit_planned_with(
        ingress,
        UnixTimestampMilliseconds::new(observed_at_ms),
        |transaction| {
            let current = core_revision(transaction)?;
            let legacy_replay = legacy_library_receipt(
                transaction,
                command_id,
                command_fingerprint,
                "read environment command",
            )?
            .is_some();
            plan_download_environment(DownloadEnvironmentActivityInput {
                command_id,
                current_revision: current,
                legacy_replay,
            })
            .map_err(|_| StorageError::InvalidActivity)
        },
        |transaction, expected, mutation| match mutation {
            DownloadEnvironmentMutation::Apply => {
                if core_revision(transaction)? != expected {
                    return Err(StorageError::RevisionConflict);
                }
                apply_download_environment(
                    transaction,
                    command_id,
                    command_fingerprint,
                    network,
                    available_capacity_bytes,
                    observed_at_ms,
                )
            }
            DownloadEnvironmentMutation::LegacyDuplicate => Ok(expected),
        },
    )?;
    Ok(receipt.committed_revision)
}

fn core_revision(connection: &rusqlite::Connection) -> Result<StateRevision, StorageError> {
    let value: i64 = connection
        .query_row(
            "SELECT state_revision FROM pod0_playback_state WHERE singleton=1",
            [],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("read environment revision", error))?;
    u64::try_from(value)
        .map(StateRevision::new)
        .map_err(|_| StorageError::InvalidActivity)
}
