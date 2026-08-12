use pod0_application::{
    ActivityDomain, ActivitySubject, DownloadDispositionActivityInput, DownloadIntentOrigin,
    DownloadInternalAdmissionActivityInput, InternalCommandKind, plan_download_internal_admission,
    plan_download_noop,
};
use pod0_domain::{CommandId, ContentDigest, EpisodeId, StateRevision, UnixTimestampMilliseconds};
use sha2::{Digest as _, Sha256};

use super::TransitionCommit;
use super::application_support::legacy_library_receipt;
use crate::{StorageError, TransitionIngress, TransitionIngressKind};

pub(crate) fn commit_download_internal_disposition(
    path: &std::path::Path,
    command: crate::PendingInternalCommand,
    episode_id: EpisodeId,
    disposition: pod0_application::RequestDisposition,
    observed_at: UnixTimestampMilliseconds,
) -> Result<crate::CommitReceipt, StorageError> {
    if !matches!(
        command.request.kind,
        InternalCommandKind::RequestEpisodeDownload { .. }
    ) || command.request.target != ActivityDomain::Download
        || command.request.episode_id != Some(episode_id)
    {
        return Err(StorageError::InvalidActivity);
    }
    let mut hash = Sha256::new();
    hash.update(b"pod0/download/internal-disposition/v1");
    hash.update(command.internal_command_id.into_bytes());
    hash.update(serde_json::to_vec(&disposition).map_err(|_| StorageError::InvalidActivity)?);
    let ingress = TransitionIngress {
        kind: TransitionIngressKind::InternalCommand,
        id: command.internal_command_id.into_bytes(),
        fingerprint: ContentDigest::from_bytes(hash.finalize().into()),
    };
    TransitionCommit::open(path)?.commit_planned_with(
        ingress,
        observed_at,
        |transaction| {
            let current = crate::download_store_read::workflow(transaction, episode_id)?
                .as_ref()
                .map_or(StateRevision::INITIAL, |record| record.workflow_revision);
            plan_download_internal_admission(DownloadInternalAdmissionActivityInput {
                internal_command_id: command.internal_command_id,
                authorizing_activity_id: command.authorizing_activity_id,
                correlation_id: command.correlation_id,
                episode_id,
                current_revision: current,
                state_changes: false,
                admitted: false,
                disposition,
            })
            .map_err(|_| StorageError::InvalidActivity)
        },
        |transaction, expected, ()| {
            let current = crate::download_store_read::workflow(transaction, episode_id)?
                .as_ref()
                .map_or(StateRevision::INITIAL, |record| record.workflow_revision);
            (current == expected)
                .then_some(expected)
                .ok_or(StorageError::RevisionConflict)
        },
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn commit_download_noop(
    path: &std::path::Path,
    command_id: CommandId,
    command_fingerprint: &str,
    subject: ActivitySubject,
    episode_id: Option<EpisodeId>,
    origin: DownloadIntentOrigin,
    internal_commands: Vec<pod0_application::DurableInternalCommandRequest>,
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
            let legacy = legacy_library_receipt(
                transaction,
                command_id,
                command_fingerprint,
                "read download disposition",
            )?;
            plan_download_noop(DownloadDispositionActivityInput {
                command_id,
                subject,
                episode_id,
                current_revision: current,
                legacy_replay: legacy.is_some(),
                origin,
                internal_commands,
            })
            .map(|plan| plan.map_mutation(|()| legacy))
            .map_err(|_| StorageError::InvalidActivity)
        },
        |transaction, expected, legacy| {
            if core_revision(transaction)? != expected {
                return Err(StorageError::RevisionConflict);
            }
            if let Some(value) = legacy {
                return Ok(value);
            }
            crate::library_store::finish_command(
                transaction,
                command_id,
                command_fingerprint,
                observed_at_ms,
            )
        },
    )?;
    Ok(receipt.committed_revision)
}

fn fingerprint(value: &str) -> Result<ContentDigest, StorageError> {
    if value.len() != 64 {
        return Err(StorageError::DownloadCommandConflict);
    }
    let mut bytes = [0_u8; 32];
    for (index, slot) in bytes.iter_mut().enumerate() {
        *slot = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)
            .map_err(|_| StorageError::DownloadCommandConflict)?;
    }
    Ok(ContentDigest::from_bytes(bytes))
}

fn revision(value: i64) -> Result<StateRevision, StorageError> {
    u64::try_from(value)
        .map(StateRevision::new)
        .map_err(|_| StorageError::InvalidActivity)
}

fn core_revision(connection: &rusqlite::Connection) -> Result<StateRevision, StorageError> {
    let value: i64 = connection
        .query_row(
            "SELECT state_revision FROM pod0_playback_state WHERE singleton=1",
            [],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("read download disposition revision", error))?;
    revision(value)
}
