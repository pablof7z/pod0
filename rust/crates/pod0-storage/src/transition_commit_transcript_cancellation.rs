use pod0_application::{TranscriptCancellationActivityInput, plan_transcript_cancellation};
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
    let current = store
        .transcript_workflow(input.episode_id)?
        .ok_or(StorageError::TranscriptWorkflowNotFound)?;
    let plan = plan_transcript_cancellation(TranscriptCancellationActivityInput {
        command_id: input.command_id,
        episode_id: input.episode_id,
        workflow_id: current.request.workflow_id,
        workflow_revision: input.expected_workflow_revision,
    })
    .map_err(|_| StorageError::InvalidActivity)?;
    TransitionCommit::open(path)?.commit_with(
        TransitionIngress {
            kind: TransitionIngressKind::ApplicationCommand,
            id: input.command_id.into_bytes(),
            fingerprint: input.command_fingerprint,
        },
        plan,
        UnixTimestampMilliseconds::new(input.observed_at_ms),
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
