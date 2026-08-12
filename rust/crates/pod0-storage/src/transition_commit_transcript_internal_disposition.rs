use pod0_application::{
    ActivityDomain, InternalCommandKind, RequestDisposition,
    TranscriptInternalDispositionActivityInput, plan_transcript_internal_disposition,
};
use pod0_domain::{ContentDigest, EpisodeId, StateRevision, UnixTimestampMilliseconds};
use sha2::{Digest as _, Sha256};

use super::TransitionCommit;
use crate::{PendingInternalCommand, StorageError, TransitionIngress, TransitionIngressKind};

pub(crate) fn commit_transcript_internal_disposition(
    path: &std::path::Path,
    command: PendingInternalCommand,
    episode_id: EpisodeId,
    state_revision: StateRevision,
    disposition: RequestDisposition,
    observed_at: UnixTimestampMilliseconds,
) -> Result<crate::CommitReceipt, StorageError> {
    let InternalCommandKind::EnsureTranscriptWorkflow { .. } = &command.request.kind else {
        return Err(StorageError::InvalidActivity);
    };
    if command.request.target != ActivityDomain::Transcript
        || command.request.episode_id != Some(episode_id)
    {
        return Err(StorageError::InvalidActivity);
    }
    let plan = plan_transcript_internal_disposition(TranscriptInternalDispositionActivityInput {
        internal_command_id: command.internal_command_id,
        authorizing_activity_id: command.authorizing_activity_id,
        correlation_id: command.correlation_id,
        episode_id,
        state_revision,
        disposition,
    })
    .map_err(|_| StorageError::InvalidActivity)?;
    let mut hash = Sha256::new();
    hash.update(b"pod0/transcript/internal-disposition/v1");
    hash.update(command.internal_command_id.into_bytes());
    hash.update(serde_json::to_vec(&disposition).map_err(|_| StorageError::InvalidActivity)?);
    TransitionCommit::open(path)?.commit_no_state_change(
        TransitionIngress {
            kind: TransitionIngressKind::InternalCommand,
            id: command.internal_command_id.into_bytes(),
            fingerprint: ContentDigest::from_bytes(hash.finalize().into()),
        },
        plan,
        observed_at,
    )
}
