use pod0_application::{
    ActivitySubject, CancellationEffectTarget, TranscriptCancellationActivityInput,
    plan_transcript_cancellation,
};
use pod0_domain::UnixTimestampMilliseconds;

use super::TransitionCommit;
use crate::{
    StorageError, TranscriptWorkflowCancellationInput, TranscriptWorkflowRecord, TransitionIngress,
    TransitionIngressKind,
};

pub(crate) fn commit_transcript_cancellation(
    path: &std::path::Path,
    input: TranscriptWorkflowCancellationInput,
) -> Result<TranscriptWorkflowRecord, StorageError> {
    let store = crate::LibraryStore::open_authoritative(path)?;
    TransitionCommit::open(path)?.commit_planned_with(
        TransitionIngress {
            kind: TransitionIngressKind::ApplicationCommand,
            id: input.command_id.into_bytes(),
            fingerprint: input.command_fingerprint,
        },
        UnixTimestampMilliseconds::new(input.observed_at_ms),
        |transaction| {
            let current = crate::transcript_workflow::read_workflow(transaction, input.episode_id)?
                .ok_or(StorageError::TranscriptWorkflowNotFound)?;
            plan_transcript_cancellation(TranscriptCancellationActivityInput {
                command_id: input.command_id,
                episode_id: input.episode_id,
                workflow_id: current.request.workflow_id,
                workflow_revision: input.expected_workflow_revision,
                target: current.request_id.map(|host_request_id| CancellationEffectTarget {
                    subject: ActivitySubject::TranscriptWorkflow {
                        workflow_id: current.request.workflow_id,
                    },
                    episode_id: Some(input.episode_id),
                    host_request_id,
                    cancellation_id: current.cancellation_id,
                }),
            })
            .map_err(|_| StorageError::InvalidActivity)
        },
        |transaction, expected, _| {
            Ok(
                crate::transcript_workflow::apply_transcript_workflow_cancellation(
                    transaction,
                    input.episode_id,
                    expected,
                    input.observed_at_ms,
                )?
                .workflow_revision,
            )
        },
    )?;
    store
        .transcript_workflow(input.episode_id)?
        .ok_or(StorageError::TranscriptWorkflowNotFound)
}
