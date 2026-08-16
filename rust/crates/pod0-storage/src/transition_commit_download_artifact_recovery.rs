use std::path::Path;

use pod0_application::{DownloadRecoveryActivityInput, DownloadTransition, plan_download_recovery};
use pod0_domain::{CommandId, ContentDigest, HostRequestId, UnixTimestampMilliseconds};
use rusqlite::params;
use sha2::{Digest as _, Sha256};

use super::TransitionCommit;
use crate::download_store_artifact::complete_request;
use crate::download_store_request::u64_to_i64;
use crate::{
    DownloadWorkflowRecord, StorageError, StoredDownloadStage, TransitionIngress,
    TransitionIngressKind,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum DownloadArtifactRecovery {
    Adopt {
        request_id: HostRequestId,
        sequence_number: u64,
        artifact_key: String,
        byte_count: u64,
        digest: [u8; 32],
    },
    RepairInvalid,
}

pub(crate) fn commit_download_artifact_recovery(
    path: &Path,
    expected: &DownloadWorkflowRecord,
    recovery: DownloadArtifactRecovery,
    now_ms: i64,
) -> Result<DownloadWorkflowRecord, StorageError> {
    let identity = identity(expected, &recovery);
    let expected = expected.clone();
    let planning_expected = expected.clone();
    let applying_recovery = recovery.clone();
    TransitionCommit::open(path)?.commit_planned_with(
        TransitionIngress {
            kind: TransitionIngressKind::Recovery,
            id: identity.into_bytes(),
            fingerprint: fingerprint(&expected, &recovery),
        },
        UnixTimestampMilliseconds::new(now_ms),
        |transaction| {
            let current = require_current(transaction, &planning_expected)?;
            validate_recovery(&current, &recovery)?;
            plan_download_recovery(DownloadRecoveryActivityInput {
                identity_command_id: identity,
                command_id: current.command_id,
                episode_id: current.episode_id,
                current_revision: current.workflow_revision,
                transition: DownloadTransition::AttemptStateChanged,
                effect: None,
            })
            .map_err(|_| StorageError::InvalidActivity)
        },
        |transaction, revision, ()| {
            let current = require_current(transaction, &expected)?;
            if current.workflow_revision != revision {
                return Err(StorageError::RevisionConflict);
            }
            apply(transaction, &current, applying_recovery, now_ms)?;
            Ok(
                crate::download_store_read::workflow(transaction, current.episode_id)?
                    .ok_or(StorageError::DownloadWorkflowNotFound)?
                    .workflow_revision,
            )
        },
    )?;
    crate::LibraryStore::open_authoritative(path)?
        .download_workflow(expected.episode_id)?
        .ok_or(StorageError::DownloadWorkflowNotFound)
}

fn require_current(
    transaction: &rusqlite::Transaction<'_>,
    expected: &DownloadWorkflowRecord,
) -> Result<DownloadWorkflowRecord, StorageError> {
    let current = crate::download_store_read::workflow(transaction, expected.episode_id)?
        .ok_or(StorageError::DownloadWorkflowNotFound)?;
    if current.workflow_revision != expected.workflow_revision || current.stage != expected.stage {
        return Err(StorageError::DownloadWorkflowConflict);
    }
    Ok(current)
}

fn validate_recovery(
    current: &DownloadWorkflowRecord,
    recovery: &DownloadArtifactRecovery,
) -> Result<(), StorageError> {
    match recovery {
        DownloadArtifactRecovery::Adopt { request_id, .. }
            if current.stage == StoredDownloadStage::Staged
                && current.request_id == Some(*request_id)
                && current.attempt_id.is_some() =>
        {
            Ok(())
        }
        DownloadArtifactRecovery::RepairInvalid
            if matches!(
                current.stage,
                StoredDownloadStage::Staged | StoredDownloadStage::Succeeded
            ) =>
        {
            Ok(())
        }
        _ => Err(StorageError::DownloadWorkflowConflict),
    }
}

fn apply(
    transaction: &rusqlite::Transaction<'_>,
    current: &DownloadWorkflowRecord,
    recovery: DownloadArtifactRecovery,
    now_ms: i64,
) -> Result<(), StorageError> {
    match recovery {
        DownloadArtifactRecovery::Adopt {
            request_id,
            sequence_number,
            artifact_key,
            byte_count,
            digest,
        } => apply_adoption(
            transaction,
            current,
            request_id,
            sequence_number,
            &artifact_key,
            byte_count,
            digest,
            now_ms,
        ),
        DownloadArtifactRecovery::RepairInvalid => apply_repair(transaction, current, now_ms),
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn apply_adoption(
    transaction: &rusqlite::Transaction<'_>,
    current: &DownloadWorkflowRecord,
    request_id: HostRequestId,
    sequence_number: u64,
    artifact_key: &str,
    byte_count: u64,
    digest: [u8; 32],
    now_ms: i64,
) -> Result<(), StorageError> {
    let attempt_id = current
        .attempt_id
        .ok_or(StorageError::StaleDownloadAttempt)?;
    let changed = transaction
        .execute(
            "UPDATE pod0_download_attempts SET state='succeeded',staged_path=NULL,\
         staged_byte_count=NULL,staged_digest=NULL,updated_at_ms=?1 WHERE attempt_id=?2 \
         AND request_id=?3 AND state='staged'",
            params![
                now_ms,
                attempt_id.into_bytes().as_slice(),
                request_id.into_bytes().as_slice()
            ],
        )
        .map_err(|error| StorageError::sqlite("recover download artifact adoption", error))?;
    if changed != 1 {
        return Err(StorageError::StaleDownloadAttempt);
    }
    complete_request(transaction, request_id, sequence_number, now_ms)?;
    let changed = transaction.execute(
        "UPDATE pod0_episodes SET download_code=2,download_wire_code=NULL,download_ref_version=1,\
         download_ref_key=?1,download_byte_count=?2 WHERE episode_id=?3",
        params![artifact_key,u64_to_i64(byte_count)?,current.episode_id.into_bytes().as_slice()],
    ).map_err(|error| StorageError::sqlite("recover episode download artifact",error))?;
    if changed != 1 {
        return Err(StorageError::EntityNotFound);
    }
    let changed = transaction.execute(
        "UPDATE pod0_download_workflows SET stage='succeeded',workflow_revision=workflow_revision+1,\
         request_id=NULL,deadline_at_ms=NULL,not_before_ms=NULL,artifact_key=?1,\
         artifact_byte_count=?2,artifact_digest=?3,failure_code=NULL,failure_detail=NULL,\
         failure_retryable=0,updated_at_ms=?4 WHERE episode_id=?5 AND request_id=?6 \
         AND attempt_id=?7 AND stage='staged' AND workflow_revision=?8",
        params![artifact_key,u64_to_i64(byte_count)?,digest.as_slice(),now_ms,
            current.episode_id.into_bytes().as_slice(),request_id.into_bytes().as_slice(),
            attempt_id.into_bytes().as_slice(),u64_to_i64(current.workflow_revision.value)?],
    ).map_err(|error| StorageError::sqlite("recover download workflow artifact",error))?;
    if changed != 1 {
        return Err(StorageError::DownloadWorkflowConflict);
    }
    Ok(())
}

pub(crate) fn apply_repair(
    transaction: &rusqlite::Transaction<'_>,
    current: &DownloadWorkflowRecord,
    now_ms: i64,
) -> Result<(), StorageError> {
    if let Some(attempt_id) = current.attempt_id {
        transaction
            .execute(
                "UPDATE pod0_download_attempts SET state='failed',failure_code='invalid_artifact',\
             failure_detail=NULL,staged_path=NULL,staged_byte_count=NULL,staged_digest=NULL,\
             updated_at_ms=?1 WHERE attempt_id=?2 AND state!='succeeded'",
                params![now_ms, attempt_id.into_bytes().as_slice()],
            )
            .map_err(|error| StorageError::sqlite("recover invalid download attempt", error))?;
    }
    if let Some(request_id) = current.request_id {
        complete_request(transaction, request_id, 0, now_ms)?;
    }
    let changed = transaction.execute(
        "UPDATE pod0_episodes SET download_code=1,download_wire_code=NULL,download_ref_version=NULL,\
         download_ref_key=NULL,download_byte_count=NULL WHERE episode_id=?1",
        [current.episode_id.into_bytes().as_slice()],
    ).map_err(|error| StorageError::sqlite("recover invalid episode download",error))?;
    if changed != 1 {
        return Err(StorageError::EntityNotFound);
    }
    let changed = transaction.execute(
        "UPDATE pod0_download_workflows SET stage='failed',workflow_revision=workflow_revision+1,\
         request_id=NULL,deadline_at_ms=NULL,not_before_ms=NULL,artifact_key=NULL,\
         artifact_byte_count=NULL,artifact_digest=NULL,failure_code='invalid_artifact',\
         failure_detail=NULL,failure_retryable=0,updated_at_ms=?1 WHERE episode_id=?2 \
         AND stage=?3 AND workflow_revision=?4",
        params![now_ms,current.episode_id.into_bytes().as_slice(),current.stage.wire(),
            u64_to_i64(current.workflow_revision.value)?],
    ).map_err(|error| StorageError::sqlite("recover invalid download workflow",error))?;
    if changed != 1 {
        return Err(StorageError::DownloadWorkflowConflict);
    }
    Ok(())
}

fn identity(record: &DownloadWorkflowRecord, recovery: &DownloadArtifactRecovery) -> CommandId {
    let digest = recovery_digest(b"pod0/download-artifact-recovery/v1", record, recovery);
    CommandId::from_bytes(digest[..16].try_into().expect("fixed digest prefix"))
}

fn fingerprint(
    record: &DownloadWorkflowRecord,
    recovery: &DownloadArtifactRecovery,
) -> ContentDigest {
    ContentDigest::from_bytes(recovery_digest(
        b"pod0/download-artifact-recovery-ingress/v1",
        record,
        recovery,
    ))
}

fn recovery_digest(
    namespace: &[u8],
    record: &DownloadWorkflowRecord,
    recovery: &DownloadArtifactRecovery,
) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(namespace);
    hash.update(record.episode_id.into_bytes());
    hash.update(record.workflow_revision.value.to_be_bytes());
    hash.update(record.stage.wire().as_bytes());
    match recovery {
        DownloadArtifactRecovery::Adopt {
            request_id,
            sequence_number,
            artifact_key,
            byte_count,
            digest,
        } => {
            hash.update([1]);
            hash.update(request_id.into_bytes());
            hash.update(sequence_number.to_be_bytes());
            hash.update(artifact_key.as_bytes());
            hash.update(byte_count.to_be_bytes());
            hash.update(digest);
        }
        DownloadArtifactRecovery::RepairInvalid => hash.update([2]),
    }
    hash.finalize().into()
}
