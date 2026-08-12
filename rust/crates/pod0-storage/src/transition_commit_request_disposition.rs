use pod0_application::{
    ActivityActor, ActivityOrigin, ActivitySubject, RequestDisposition,
    RequestDispositionActivityInput, plan_request_disposition,
};
use pod0_domain::{CommandId, ContentDigest, EpisodeId, StateRevision, UnixTimestampMilliseconds};

use super::TransitionCommit;
use crate::{StorageError, TransitionIngress, TransitionIngressKind};

#[allow(clippy::too_many_arguments)]
pub(crate) fn commit_request_disposition(
    path: &std::path::Path,
    command_id: CommandId,
    fingerprint: ContentDigest,
    subject: ActivitySubject,
    episode_id: Option<EpisodeId>,
    actor: ActivityActor,
    origin: ActivityOrigin,
    disposition: RequestDisposition,
    observed_at: UnixTimestampMilliseconds,
) -> Result<crate::CommitReceipt, StorageError> {
    let ingress = TransitionIngress {
        kind: TransitionIngressKind::ApplicationCommand,
        id: command_id.into_bytes(),
        fingerprint,
    };
    TransitionCommit::open(path)?.commit_planned_with(
        ingress,
        observed_at,
        |connection| {
            let value: i64 = connection
                .query_row(
                    "SELECT state_revision FROM pod0_playback_state WHERE singleton=1",
                    [],
                    |row| row.get(0),
                )
                .map_err(|error| {
                    StorageError::sqlite("read request disposition revision", error)
                })?;
            let current = u64::try_from(value)
                .map(StateRevision::new)
                .map_err(|_| StorageError::InvalidActivity)?;
            plan_request_disposition(RequestDispositionActivityInput {
                command_id,
                subject,
                episode_id,
                current_revision: current,
                actor,
                origin,
                disposition,
            })
            .map_err(|_| StorageError::InvalidActivity)
        },
        |_, expected, ()| Ok(expected),
    )
}
