use pod0_application::{
    InternalCommandOwnerActivityInput, RequestDisposition, plan_internal_command_owner_activity,
};
use pod0_domain::{CommandId, ContentDigest, StateRevision, UnixTimestampMilliseconds};
use sha2::{Digest as _, Sha256};

use super::TransitionCommit;
use crate::{PendingInternalCommand, StorageError, TransitionIngress, TransitionIngressKind};

pub(crate) fn commit_internal_command_disposition(
    path: &std::path::Path,
    command: PendingInternalCommand,
    state_revision: StateRevision,
    disposition: RequestDisposition,
    observed_at: UnixTimestampMilliseconds,
) -> Result<crate::CommitReceipt, StorageError> {
    if disposition == RequestDisposition::Accepted {
        return Err(StorageError::InvalidActivity);
    }
    let mut hash = Sha256::new();
    hash.update(b"pod0/internal-command/disposition/v1");
    hash.update(command.internal_command_id.into_bytes());
    hash.update(serde_json::to_vec(&disposition).map_err(|_| StorageError::InvalidActivity)?);
    TransitionCommit::open(path)?.commit_planned_with(
        TransitionIngress {
            kind: TransitionIngressKind::InternalCommand,
            id: command.internal_command_id.into_bytes(),
            fingerprint: ContentDigest::from_bytes(hash.finalize().into()),
        },
        observed_at,
        |_| {
            plan_internal_command_owner_activity(InternalCommandOwnerActivityInput {
                internal_command_id: command.internal_command_id,
                authorizing_activity_id: command.authorizing_activity_id,
                correlation_id: command.correlation_id,
                command_id: CommandId::from_bytes(command.internal_command_id.into_bytes()),
                subject: command.request.subject,
                episode_id: command.request.episode_id,
                current_revision: state_revision,
                committed_revision: state_revision,
                disposition,
                transitions: Vec::new(),
                effects: Vec::new(),
                internal_commands: Vec::new(),
            })
            .map_err(|_| StorageError::InvalidActivity)
        },
        |_, expected, _| Ok(expected),
    )
}
