use std::path::Path;

use pod0_application::{
    ActivityDomain, ActivitySubject, DownloadFinalizationActivityInput, InternalCommandKind,
    plan_download_finalization,
};
use pod0_domain::{ContentDigest, HostRequestId, UnixTimestampMilliseconds};
use sha2::{Digest as _, Sha256};

use super::TransitionCommit;
use crate::download_store_artifact_file::{
    artifact_key, copy_and_hash_staged, install_staged, sync_parent,
};
use crate::{
    DownloadWorkflowRecord, LibraryStore, PendingInternalCommand, StorageError, TransitionIngress,
    TransitionIngressKind,
};

pub(super) enum Finalization {
    Succeeded {
        artifact_key: String,
        byte_count: u64,
        digest: [u8; 32],
    },
    InvalidArtifact,
}

impl LibraryStore {
    pub fn finalize_pending_download_artifact(
        &self,
        request_id: HostRequestId,
        committed_at: UnixTimestampMilliseconds,
    ) -> Result<Option<DownloadWorkflowRecord>, StorageError> {
        let Some(command) = self
            .pending_download_finalization_commands(100)?
            .into_iter()
            .find(|command| finalization_request(command) == Some(request_id))
        else {
            return Ok(None);
        };
        let episode_id = self
            .download_host_request(request_id)?
            .map(|value| value.0.episode_id)
            .ok_or(StorageError::DownloadRequestNotFound)?;
        let current = self
            .download_workflow(episode_id)?
            .ok_or(StorageError::DownloadWorkflowNotFound)?;
        let (path, claimed_byte_count) = validate_command(&command, &current, request_id)?;
        let attempt_id = current
            .attempt_id
            .ok_or(StorageError::StaleDownloadAttempt)?;
        let finalization = match copy_and_hash_staged(
            self.path(),
            Path::new(path),
            attempt_id,
            claimed_byte_count,
        ) {
            Ok(staged) => {
                let key = artifact_key(current.intent_id, current.attempt, staged.digest);
                let final_path = self.download_artifact_path(&key)?;
                install_staged(
                    &staged.pending_path,
                    &final_path,
                    staged.byte_count,
                    staged.digest,
                )?;
                sync_parent(&final_path)?;
                Finalization::Succeeded {
                    artifact_key: key,
                    byte_count: staged.byte_count,
                    digest: staged.digest,
                }
            }
            Err(StorageError::InvalidDownloadArtifact) => Finalization::InvalidArtifact,
            Err(error) => return Err(error),
        };
        commit(self.path(), command, request_id, finalization, committed_at)?;
        Ok(self.download_workflow(current.episode_id)?)
    }
}

fn commit(
    path: &Path,
    command: PendingInternalCommand,
    request_id: HostRequestId,
    finalization: Finalization,
    committed_at: UnixTimestampMilliseconds,
) -> Result<(), StorageError> {
    let episode_id = command
        .request
        .episode_id
        .ok_or(StorageError::InvalidActivity)?;
    let fingerprint = fingerprint(&command, &finalization);
    TransitionCommit::open(path)?.commit_planned_with(
        TransitionIngress {
            kind: TransitionIngressKind::InternalCommand,
            id: command.internal_command_id.into_bytes(),
            fingerprint,
        },
        committed_at,
        |transaction| {
            let current = crate::download_store_read::workflow(transaction, episode_id)?
                .ok_or(StorageError::DownloadWorkflowNotFound)?;
            if current.request_id != Some(request_id)
                || current.stage != crate::StoredDownloadStage::Transferring
            {
                return Err(StorageError::DownloadWorkflowConflict);
            }
            plan_download_finalization(DownloadFinalizationActivityInput {
                internal_command_id: command.internal_command_id,
                authorizing_activity_id: command.authorizing_activity_id,
                correlation_id: command.correlation_id,
                command_id: current.command_id,
                request_id,
                episode_id,
                current_revision: current.workflow_revision,
            })
            .map_err(|_| StorageError::InvalidActivity)
        },
        |transaction, expected, ()| {
            let current = crate::download_store_read::workflow(transaction, episode_id)?
                .ok_or(StorageError::DownloadWorkflowNotFound)?;
            if current.workflow_revision != expected || current.request_id != Some(request_id) {
                return Err(StorageError::RevisionConflict);
            }
            super::download_finalization_apply::apply(
                transaction,
                &current,
                request_id,
                finalization,
                committed_at.value,
            )
        },
    )?;
    Ok(())
}

fn validate_command<'a>(
    command: &'a PendingInternalCommand,
    current: &DownloadWorkflowRecord,
    request_id: HostRequestId,
) -> Result<(&'a str, u64), StorageError> {
    let InternalCommandKind::FinalizeDownloadArtifact {
        request_id: owned_request,
        staged_file_path,
        claimed_byte_count,
        ..
    } = &command.request.kind
    else {
        return Err(StorageError::InvalidActivity);
    };
    if *owned_request != request_id
        || command.request.target != ActivityDomain::Download
        || command.request.episode_id != Some(current.episode_id)
        || command.request.subject
            != (ActivitySubject::Episode {
                episode_id: current.episode_id,
            })
    {
        return Err(StorageError::InvalidActivity);
    }
    Ok((staged_file_path, *claimed_byte_count))
}

fn finalization_request(command: &PendingInternalCommand) -> Option<HostRequestId> {
    match command.request.kind {
        InternalCommandKind::FinalizeDownloadArtifact { request_id, .. } => Some(request_id),
        _ => None,
    }
}

fn fingerprint(command: &PendingInternalCommand, finalization: &Finalization) -> ContentDigest {
    let mut hash = Sha256::new();
    hash.update(b"pod0/download-artifact-finalization/v1");
    hash.update(command.internal_command_id.into_bytes());
    match finalization {
        Finalization::Succeeded {
            artifact_key,
            byte_count,
            digest,
        } => {
            hash.update([1]);
            hash.update(artifact_key.as_bytes());
            hash.update(byte_count.to_be_bytes());
            hash.update(digest);
        }
        Finalization::InvalidArtifact => hash.update([2]),
    }
    ContentDigest::from_bytes(hash.finalize().into())
}
