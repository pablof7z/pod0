use pod0_application::{
    DownloadCutoverActivityInput, DownloadCutoverMutation, RequestDisposition,
    RequestRejectionReason, plan_download_cutover,
};
use pod0_domain::{CommandId, ContentDigest, StateRevision, UnixTimestampMilliseconds};
use sha2::{Digest as _, Sha256};

use super::TransitionCommit;
use crate::download_store_cutover::{CUTOVER_DOMAIN, read_authority};
use crate::{
    DownloadWorkflowAuthorityState, StorageError, TransitionIngress, TransitionIngressKind,
};

pub(crate) fn commit_download_cutover(
    path: &std::path::Path,
    source_generation: u64,
    committed_at_ms: i64,
) -> Result<DownloadWorkflowAuthorityState, StorageError> {
    if source_generation == 0 || committed_at_ms < 0 {
        return Err(StorageError::DownloadWorkflowConflict);
    }
    let command_id = command_id(source_generation);
    let receipt = TransitionCommit::open(path)?.commit_planned_with(
        TransitionIngress {
            kind: TransitionIngressKind::Migration,
            id: command_id.into_bytes(),
            fingerprint: fingerprint(source_generation),
        },
        UnixTimestampMilliseconds::new(committed_at_ms),
        |transaction| {
            let current = core_revision(transaction)?;
            let disposition = match read_authority(transaction)? {
                DownloadWorkflowAuthorityState::Staged {
                    source_generation: staged,
                } if staged == source_generation => {
                    verify_staged(transaction)?;
                    RequestDisposition::Accepted
                }
                DownloadWorkflowAuthorityState::Authoritative {
                    source_generation: current,
                } if current == source_generation => RequestDisposition::AlreadyComplete,
                _ => RequestDisposition::Rejected {
                    reason: RequestRejectionReason::MissingPrerequisite,
                },
            };
            plan_download_cutover(DownloadCutoverActivityInput {
                command_id,
                current_revision: current,
                committed_revision: if disposition == RequestDisposition::Accepted {
                    StateRevision::new(
                        current
                            .value
                            .checked_add(1)
                            .ok_or(StorageError::InvalidActivity)?,
                    )
                } else {
                    current
                },
                disposition,
            })
            .map_err(|_| StorageError::InvalidActivity)
        },
        |transaction, expected, mutation| match mutation {
            DownloadCutoverMutation::Apply => {
                require_core_revision(transaction, expected)?;
                transaction
                    .execute(
                        "UPDATE pod0_episodes SET download_code=1,download_wire_code=NULL,\
                         download_ref_version=NULL,download_ref_key=NULL,download_byte_count=NULL \
                         WHERE episode_id IN(SELECT episode_id FROM pod0_download_workflows \
                         WHERE stage='requested')",
                        [],
                    )
                    .map_err(|error| {
                        StorageError::sqlite("clear restarted legacy downloads", error)
                    })?;
                let committed = crate::library_store::advance_playback_revision(transaction)?;
                let value =
                    i64::try_from(committed.value).map_err(|_| StorageError::InvalidActivity)?;
                let changed = transaction
                    .execute(
                        "UPDATE pod0_domain_cutovers SET state='authoritative',core_revision=?1,\
                         committed_at_ms=?2 WHERE domain=?3 AND state='staged' \
                         AND source_generation=?4",
                        rusqlite::params![
                            value,
                            committed_at_ms,
                            CUTOVER_DOMAIN,
                            crate::download_store_request::u64_to_i64(source_generation)?
                        ],
                    )
                    .map_err(|error| StorageError::sqlite("commit download cutover", error))?;
                (changed == 1)
                    .then_some(committed)
                    .ok_or(StorageError::RevisionConflict)
            }
            DownloadCutoverMutation::None => {
                require_core_revision(transaction, expected)?;
                Ok(expected)
            }
        },
    )?;
    match receipt.disposition {
        RequestDisposition::Accepted
        | RequestDisposition::AlreadyComplete
        | RequestDisposition::Duplicate => {
            Ok(DownloadWorkflowAuthorityState::Authoritative { source_generation })
        }
        RequestDisposition::Rejected { .. } => Err(StorageError::DownloadWorkflowConflict),
        _ => Err(StorageError::InvalidActivity),
    }
}

fn verify_staged(transaction: &rusqlite::Transaction<'_>) -> Result<(), StorageError> {
    let invalid: bool = transaction
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM pod0_download_workflows \
             WHERE stage NOT IN('succeeded','requested'))",
            [],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("verify staged download cutover", error))?;
    (!invalid)
        .then_some(())
        .ok_or(StorageError::DownloadWorkflowConflict)
}

fn command_id(source_generation: u64) -> CommandId {
    let mut hash = Sha256::new();
    hash.update(b"pod0-download-cutover-command-v1");
    hash.update(source_generation.to_be_bytes());
    let digest: [u8; 32] = hash.finalize().into();
    CommandId::from_bytes(digest[..16].try_into().expect("digest prefix"))
}

fn fingerprint(source_generation: u64) -> ContentDigest {
    let mut hash = Sha256::new();
    hash.update(b"pod0-download-cutover-ingress-v1");
    hash.update(source_generation.to_be_bytes());
    ContentDigest::from_bytes(hash.finalize().into())
}

fn core_revision(connection: &rusqlite::Connection) -> Result<StateRevision, StorageError> {
    let value: i64 = connection
        .query_row(
            "SELECT state_revision FROM pod0_playback_state WHERE singleton=1",
            [],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("read download cutover revision", error))?;
    super::application_support::revision(value)
}

fn require_core_revision(
    transaction: &rusqlite::Transaction<'_>,
    expected: StateRevision,
) -> Result<(), StorageError> {
    (core_revision(transaction)? == expected)
        .then_some(())
        .ok_or(StorageError::RevisionConflict)
}
