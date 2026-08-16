use pod0_application::{
    ActivityDomain, InternalCommandKind, RequestDisposition, TranscriptAdmissionActivityInput,
    TranscriptAdmissionMutation, TranscriptDispositionActivityInput,
    TranscriptInternalAdmissionActivityInput, TranscriptWorkflowOrigin, plan_transcript_admission,
    plan_transcript_internal_admission, plan_transcript_request_disposition,
};

impl crate::LibraryStore {
    #[allow(clippy::too_many_arguments)]
    pub fn record_transcript_request_disposition(
        &self,
        command_id: pod0_domain::CommandId,
        fingerprint: ContentDigest,
        episode_id: pod0_domain::EpisodeId,
        state_revision: pod0_domain::StateRevision,
        origin: TranscriptWorkflowOrigin,
        disposition: RequestDisposition,
        observed_at: UnixTimestampMilliseconds,
    ) -> Result<crate::CommitReceipt, StorageError> {
        TransitionCommit::open(self.path())?.commit_planned_with(
            TransitionIngress {
                kind: TransitionIngressKind::ApplicationCommand,
                id: command_id.into_bytes(),
                fingerprint,
            },
            observed_at,
            |_| {
                plan_transcript_request_disposition(TranscriptDispositionActivityInput {
                    command_id,
                    episode_id,
                    state_revision,
                    origin,
                    disposition,
                })
                .map_err(|_| StorageError::InvalidActivity)
            },
            |_, expected, ()| Ok(expected),
        )
    }
}
use pod0_domain::{ContentDigest, UnixTimestampMilliseconds};
use sha2::{Digest as _, Sha256};

use super::TransitionCommit;
use crate::{
    StorageError, TranscriptWorkflowEnsureInput, TranscriptWorkflowEnsureOutcome,
    TransitionIngress, TransitionIngressKind, apply_transcript_workflow_ensure, replays,
    validate_ensure,
};

pub(crate) fn commit_transcript_admission(
    path: &std::path::Path,
    input: TranscriptWorkflowEnsureInput,
    fingerprint: ContentDigest,
) -> Result<TranscriptWorkflowEnsureOutcome, StorageError> {
    validate_ensure(&input)?;
    let store = crate::LibraryStore::open_authoritative(path)?;
    let origin = origin(&input.request.origin)?;
    let receipt = TransitionCommit::open(path)?.commit_planned_with(
        TransitionIngress {
            kind: TransitionIngressKind::ApplicationCommand,
            id: input.command_id.into_bytes(),
            fingerprint,
        },
        UnixTimestampMilliseconds::new(input.now_ms),
        |transaction| {
            let existing =
                crate::transcript_workflow::read_workflow(transaction, input.episode_id)?;
            plan_transcript_admission(TranscriptAdmissionActivityInput {
                command_id: input.command_id,
                episode_id: input.episode_id,
                workflow_id: input.request.workflow_id,
                current_workflow_revision: existing.as_ref().map(|record| record.workflow_revision),
                exact_replay: existing
                    .as_ref()
                    .is_some_and(|record| replays(record, &input)),
                origin,
            })
            .map_err(|_| StorageError::InvalidActivity)
        },
        |transaction, expected, mutation| match mutation {
            TranscriptAdmissionMutation::Ensure => {
                Ok(
                    apply_transcript_workflow_ensure(transaction, input.clone(), expected)?
                        .workflow_revision,
                )
            }
            TranscriptAdmissionMutation::RecordDuplicate => {
                let record =
                    crate::transcript_workflow::read_workflow(transaction, input.episode_id)?
                        .ok_or(StorageError::TranscriptWorkflowNotFound)?;
                if record.workflow_revision != expected || !replays(&record, &input) {
                    return Err(StorageError::RevisionConflict);
                }
                Ok(expected)
            }
        },
    )?;
    let record = store
        .transcript_workflow(input.episode_id)?
        .ok_or(StorageError::TranscriptWorkflowNotFound)?;
    if receipt.replayed || receipt.disposition == RequestDisposition::Duplicate {
        Ok(TranscriptWorkflowEnsureOutcome::Existing(record))
    } else {
        Ok(TranscriptWorkflowEnsureOutcome::Changed(record))
    }
}

pub(crate) fn commit_transcript_internal_admission(
    path: &std::path::Path,
    command: crate::PendingInternalCommand,
    input: TranscriptWorkflowEnsureInput,
) -> Result<TranscriptWorkflowEnsureOutcome, StorageError> {
    validate_ensure(&input)?;
    let fingerprint = transcript_admission_fingerprint(&input);
    let InternalCommandKind::EnsureTranscriptWorkflow {
        origin: authorized_origin,
        ..
    } = &command.request.kind
    else {
        return Err(StorageError::InvalidActivity);
    };
    if command.request.target != ActivityDomain::Transcript
        || command.request.episode_id != Some(input.episode_id)
        || *authorized_origin != origin(&input.request.origin)?
        || command.internal_command_id.into_bytes() != input.command_id.into_bytes()
    {
        return Err(StorageError::InvalidActivity);
    }
    let store = crate::LibraryStore::open_authoritative(path)?;
    let receipt = TransitionCommit::open(path)?.commit_planned_with(
        TransitionIngress {
            kind: TransitionIngressKind::InternalCommand,
            id: command.internal_command_id.into_bytes(),
            fingerprint,
        },
        UnixTimestampMilliseconds::new(input.now_ms),
        |transaction| {
            let existing =
                crate::transcript_workflow::read_workflow(transaction, input.episode_id)?;
            plan_transcript_internal_admission(TranscriptInternalAdmissionActivityInput {
                internal_command_id: command.internal_command_id,
                authorizing_activity_id: command.authorizing_activity_id,
                correlation_id: command.correlation_id,
                episode_id: input.episode_id,
                workflow_id: input.request.workflow_id,
                current_workflow_revision: existing.as_ref().map(|record| record.workflow_revision),
                exact_replay: existing
                    .as_ref()
                    .is_some_and(|record| replays(record, &input)),
            })
            .map_err(|_| StorageError::InvalidActivity)
        },
        |transaction, expected, mutation| match mutation {
            TranscriptAdmissionMutation::Ensure => {
                Ok(
                    apply_transcript_workflow_ensure(transaction, input.clone(), expected)?
                        .workflow_revision,
                )
            }
            TranscriptAdmissionMutation::RecordDuplicate => {
                let record =
                    crate::transcript_workflow::read_workflow(transaction, input.episode_id)?
                        .ok_or(StorageError::TranscriptWorkflowNotFound)?;
                if record.workflow_revision != expected || !replays(&record, &input) {
                    return Err(StorageError::RevisionConflict);
                }
                Ok(expected)
            }
        },
    )?;
    let record = store
        .transcript_workflow(input.episode_id)?
        .ok_or(StorageError::TranscriptWorkflowNotFound)?;
    if receipt.replayed || receipt.disposition == RequestDisposition::Duplicate {
        Ok(TranscriptWorkflowEnsureOutcome::Existing(record))
    } else {
        Ok(TranscriptWorkflowEnsureOutcome::Changed(record))
    }
}

fn origin(value: &str) -> Result<TranscriptWorkflowOrigin, StorageError> {
    match value {
        "user" => Ok(TranscriptWorkflowOrigin::User),
        "automatic" => Ok(TranscriptWorkflowOrigin::Automatic),
        "playback" => Ok(TranscriptWorkflowOrigin::Playback),
        _ => Err(StorageError::InvalidActivity),
    }
}

pub(crate) fn transcript_admission_fingerprint(
    input: &TranscriptWorkflowEnsureInput,
) -> ContentDigest {
    let mut hash = Sha256::new();
    hash.update(b"pod0/transcript/admission/v1");
    hash.update(input.episode_id.into_bytes());
    hash.update(input.request.workflow_id.into_bytes());
    for value in [
        &input.request.source_revision,
        &input.request.origin,
        &input.request.provider,
        &input.request.model,
        &input.request.remote_audio_url,
    ] {
        hash.update(value.as_bytes());
        hash.update([0]);
    }
    for value in [
        input.request.local_audio_url.as_deref(),
        input.request.publisher_transcript_url.as_deref(),
        input.request.publisher_mime_hint.as_deref(),
    ] {
        hash.update(value.unwrap_or_default().as_bytes());
        hash.update([u8::from(value.is_some())]);
    }
    hash.update([
        u8::from(input.request.publisher_first),
        u8::from(input.request.provider_fallback_enabled),
        stage_code(input.stage),
    ]);
    if let Some(attempt) = input.prepared_attempt {
        hash.update([1]);
        hash.update(attempt.attempt.to_be_bytes());
        hash.update(attempt.attempt_id.into_bytes());
        hash.update(attempt.submission_fence_id.into_bytes());
    } else {
        hash.update([0]);
    }
    hash.update(input.cancellation_id.into_bytes());
    if let Some(request_id) = input.request_id {
        hash.update([1]);
        hash.update(request_id.into_bytes());
    } else {
        hash.update([0]);
    }
    hash.update(input.issued_revision.value.to_be_bytes());
    hash.update(input.deadline_at_ms.unwrap_or(i64::MIN).to_be_bytes());
    hash.update(input.expected_selection_revision.value.to_be_bytes());
    hash.update(input.max_attempts.to_be_bytes());
    hash.update(
        input
            .expected_workflow_revision
            .map_or(u64::MAX, |revision| revision.value)
            .to_be_bytes(),
    );
    ContentDigest::from_bytes(hash.finalize().into())
}

const fn stage_code(stage: crate::StoredTranscriptWorkflowStage) -> u8 {
    use crate::StoredTranscriptWorkflowStage as Stage;
    match stage {
        Stage::AwaitingPrerequisite => 1,
        Stage::Requested => 2,
        Stage::PublisherRequested => 3,
        Stage::SubmissionAuthorized => 4,
        Stage::ProviderAccepted => 5,
        Stage::CompletionObserved => 6,
        Stage::TranscriptCommitted => 7,
        Stage::EvidenceRequested => 8,
        Stage::RetryScheduled => 9,
        Stage::Blocked => 10,
        Stage::Failed => 11,
        Stage::Cancelled => 12,
        Stage::Succeeded => 13,
    }
}
