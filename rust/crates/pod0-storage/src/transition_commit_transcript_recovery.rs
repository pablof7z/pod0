use pod0_application::{TranscriptAmbiguousRecoveryInput, plan_transcript_ambiguous_recovery};
use pod0_domain::{CommandId, ContentDigest, EpisodeId, HostRequestId, StateRevision};
use sha2::{Digest as _, Sha256};

use super::TransitionCommit;
use crate::{StorageError, TransitionIngress, TransitionIngressKind};

pub(crate) fn commit_transcript_ambiguous_recovery(
    path: &std::path::Path,
    episode_id: EpisodeId,
    request_id: HostRequestId,
    now_ms: i64,
) -> Result<(), StorageError> {
    TransitionCommit::open(path)?.commit_resolved_ingress_with(
        pod0_domain::UnixTimestampMilliseconds::new(now_ms),
        |transaction| {
            let record = exact_authorized(transaction, episode_id, request_id)?;
            Ok(TransitionIngress {
                kind: TransitionIngressKind::Recovery,
                id: recovery_id(request_id, record.workflow_revision).into_bytes(),
                fingerprint: fingerprint(request_id, record.workflow_revision),
            })
        },
        |transaction| {
            let record = exact_authorized(transaction, episode_id, request_id)?;
            plan_transcript_ambiguous_recovery(TranscriptAmbiguousRecoveryInput {
                recovery_id: recovery_id(request_id, record.workflow_revision),
                command_id: record.command_id,
                request_id,
                episode_id,
                workflow_id: record.request.workflow_id,
                current_revision: record.workflow_revision,
            })
            .map_err(|_| StorageError::InvalidActivity)
        },
        |transaction, expected, ()| {
            crate::transcript_workflow::apply_ambiguous_recovery(
                transaction, episode_id, request_id, expected, now_ms,
            )
        },
    )?;
    Ok(())
}

fn exact_authorized(
    connection: &rusqlite::Connection,
    episode_id: EpisodeId,
    request_id: HostRequestId,
) -> Result<crate::TranscriptWorkflowRecord, StorageError> {
    crate::transcript_workflow::read_workflow(connection, episode_id)?
        .filter(|record| {
            record.request_id == Some(request_id)
                && record.stage == crate::StoredTranscriptWorkflowStage::SubmissionAuthorized
        })
        .ok_or(StorageError::StaleTranscriptAttempt)
}

fn recovery_id(request_id: HostRequestId, revision: StateRevision) -> CommandId {
    let digest = recovery_digest(request_id, revision);
    CommandId::from_bytes(digest[..16].try_into().expect("fixed digest prefix"))
}

fn fingerprint(request_id: HostRequestId, revision: StateRevision) -> ContentDigest {
    ContentDigest::from_bytes(recovery_digest(request_id, revision))
}

fn recovery_digest(request_id: HostRequestId, revision: StateRevision) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(b"pod0/transcript/ambiguous-recovery/v1");
    hash.update(request_id.into_bytes());
    hash.update(revision.value.to_be_bytes());
    hash.finalize().into()
}
