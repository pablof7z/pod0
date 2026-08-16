use pod0_application::{
    TranscriptEffectActivityIdentity, TranscriptSubmissionActivityInput, TranscriptWorkflowOrigin,
    plan_transcript_publisher_effect, plan_transcript_recovery_effect, plan_transcript_submission,
};
use pod0_domain::{
    ContentDigest, EpisodeId, HostRequestId, StateRevision, UnixTimestampMilliseconds,
};
use sha2::{Digest as _, Sha256};

use super::TransitionCommit;
use crate::{
    StorageError, TranscriptSubmissionClaim, TranscriptSubmissionClaimInput, TransitionIngress,
    TransitionIngressKind, authorize_submission, exact_attempt, require_authoritative,
    validate_claim,
};

pub(crate) fn commit_transcript_submission(
    path: &std::path::Path,
    input: TranscriptSubmissionClaimInput,
) -> Result<TranscriptSubmissionClaim, StorageError> {
    let store = crate::LibraryStore::open_authoritative(path)?;
    let receipt = TransitionCommit::open(path)?.commit_planned_with(
        TransitionIngress {
            kind: TransitionIngressKind::ScheduledWake,
            id: input.request_id.into_bytes(),
            fingerprint: fingerprint(input),
        },
        UnixTimestampMilliseconds::new(input.now_ms),
        |transaction| {
            let current = exact_submission_record(transaction, input)?;
            if current.stage.protects_submission() {
                return Err(StorageError::TranscriptWorkflowConflict);
            }
            plan_transcript_submission(TranscriptSubmissionActivityInput {
                request_id: input.request_id,
                command_id: current.command_id,
                episode_id: current.episode_id,
                workflow_id: current.request.workflow_id,
                workflow_revision: current.workflow_revision,
                origin: origin(&current.request.origin)?,
                deadline_at: current.deadline_at_ms.map(UnixTimestampMilliseconds::new),
                execution: crate::transcript_effect_request::exact_transcript_request(
                    transaction,
                    &current,
                    true,
                )?,
            })
            .map_err(|_| StorageError::InvalidActivity)
        },
        |transaction, expected, _| {
            require_authoritative(transaction)?;
            let record = exact_attempt(
                transaction,
                input.episode_id,
                input.request_id,
                input.attempt_id,
                input.submission_fence_id,
            )?;
            if record.workflow_revision != expected {
                return Err(StorageError::RevisionConflict);
            }
            validate_claim(&record, &input)?;
            authorize_submission(transaction, &input)?;
            Ok(StateRevision::new(
                expected
                    .value
                    .checked_add(1)
                    .ok_or(StorageError::InvalidActivity)?,
            ))
        },
    )?;
    let record = store
        .transcript_workflow(input.episode_id)?
        .ok_or(StorageError::TranscriptWorkflowNotFound)?;
    if receipt.replayed {
        Ok(TranscriptSubmissionClaim::AlreadyClaimed(record))
    } else {
        Ok(TranscriptSubmissionClaim::Authorized(record))
    }
}

pub(crate) fn commit_transcript_publisher_effect(
    path: &std::path::Path,
    episode_id: EpisodeId,
    request_id: HostRequestId,
    now_ms: i64,
) -> Result<crate::TranscriptWorkflowRecord, StorageError> {
    let store = crate::LibraryStore::open_authoritative(path)?;
    TransitionCommit::open(path)?.commit_resolved_ingress_with(
        UnixTimestampMilliseconds::new(now_ms),
        |transaction| {
            let current = publisher_record(transaction, episode_id, request_id)?;
            Ok(TransitionIngress {
                kind: TransitionIngressKind::ScheduledWake,
                id: TranscriptEffectActivityIdentity::new(request_id, current.workflow_revision)
                    .effect_intent_id(0)
                    .into_bytes(),
                fingerprint: publisher_fingerprint(&current),
            })
        },
        |transaction| {
            let current = publisher_record(transaction, episode_id, request_id)?;
            plan_transcript_publisher_effect(activity_input(transaction, &current, request_id, true)?)
                .map_err(|_| StorageError::InvalidActivity)
        },
        |transaction, expected, ()| {
            require_authoritative(transaction)?;
            let record = crate::transcript_workflow::read_workflow(transaction, episode_id)?
                .ok_or(StorageError::TranscriptWorkflowNotFound)?;
            if record.request_id != Some(request_id)
                || record.workflow_revision != expected
                || record.stage != crate::StoredTranscriptWorkflowStage::PublisherRequested
            {
                return Err(StorageError::StaleTranscriptAttempt);
            }
            Ok(expected)
        },
    )?;
    store
        .transcript_workflow(episode_id)?
        .ok_or(StorageError::TranscriptWorkflowNotFound)
}

pub(crate) fn commit_transcript_recovery_effect(
    path: &std::path::Path,
    episode_id: EpisodeId,
    request_id: HostRequestId,
    now_ms: i64,
) -> Result<crate::TranscriptWorkflowRecord, StorageError> {
    let store = crate::LibraryStore::open_authoritative(path)?;
    TransitionCommit::open(path)?.commit_resolved_ingress_with(
        UnixTimestampMilliseconds::new(now_ms),
        |transaction| {
            let current = recovery_record(transaction, episode_id, request_id, now_ms)?;
            Ok(TransitionIngress {
                kind: TransitionIngressKind::Recovery,
                id: TranscriptEffectActivityIdentity::new(request_id, current.workflow_revision)
                    .effect_intent_id(0)
                    .into_bytes(),
                fingerprint: recovery_fingerprint(&current),
            })
        },
        |transaction| {
            let current = recovery_record(transaction, episode_id, request_id, now_ms)?;
            plan_transcript_recovery_effect(activity_input(transaction, &current, request_id, false)?)
                .map_err(|_| StorageError::InvalidActivity)
        },
        |transaction, expected, ()| {
            require_authoritative(transaction)?;
            let record = crate::transcript_workflow::read_workflow(transaction, episode_id)?
                .ok_or(StorageError::TranscriptWorkflowNotFound)?;
            if record.request_id != Some(request_id)
                || record.workflow_revision != expected
                || record.stage != crate::StoredTranscriptWorkflowStage::ProviderAccepted
            {
                return Err(StorageError::StaleTranscriptAttempt);
            }
            Ok(expected)
        },
    )?;
    store
        .transcript_workflow(episode_id)?
        .ok_or(StorageError::TranscriptWorkflowNotFound)
}

fn exact_submission_record(
    transaction: &rusqlite::Transaction<'_>,
    input: TranscriptSubmissionClaimInput,
) -> Result<crate::TranscriptWorkflowRecord, StorageError> {
    let current = crate::transcript_workflow::read_workflow(transaction, input.episode_id)?
        .ok_or(StorageError::TranscriptWorkflowNotFound)?;
    if current.request_id != Some(input.request_id)
        || current.attempt_id != Some(input.attempt_id)
        || current.submission_fence_id != Some(input.submission_fence_id)
    {
        return Err(StorageError::StaleTranscriptAttempt);
    }
    Ok(current)
}

fn publisher_record(
    transaction: &rusqlite::Transaction<'_>,
    episode_id: EpisodeId,
    request_id: HostRequestId,
) -> Result<crate::TranscriptWorkflowRecord, StorageError> {
    let current = exact_request_record(transaction, episode_id, request_id)?;
    if current.stage != crate::StoredTranscriptWorkflowStage::PublisherRequested {
        return Err(StorageError::TranscriptWorkflowConflict);
    }
    Ok(current)
}

fn recovery_record(
    transaction: &rusqlite::Transaction<'_>,
    episode_id: EpisodeId,
    request_id: HostRequestId,
    now_ms: i64,
) -> Result<crate::TranscriptWorkflowRecord, StorageError> {
    let current = exact_request_record(transaction, episode_id, request_id)?;
    if current.stage != crate::StoredTranscriptWorkflowStage::ProviderAccepted
        || current.not_before_ms.is_some_and(|value| value > now_ms)
    {
        return Err(StorageError::TranscriptWorkflowConflict);
    }
    Ok(current)
}

fn exact_request_record(
    transaction: &rusqlite::Transaction<'_>,
    episode_id: EpisodeId,
    request_id: HostRequestId,
) -> Result<crate::TranscriptWorkflowRecord, StorageError> {
    crate::transcript_workflow::read_workflow(transaction, episode_id)?
        .filter(|record| record.request_id == Some(request_id))
        .ok_or(StorageError::StaleTranscriptAttempt)
}

fn activity_input(
    transaction: &rusqlite::Transaction<'_>,
    current: &crate::TranscriptWorkflowRecord,
    request_id: HostRequestId,
    include_deadline: bool,
) -> Result<TranscriptSubmissionActivityInput, StorageError> {
    Ok(TranscriptSubmissionActivityInput {
        request_id,
        command_id: current.command_id,
        episode_id: current.episode_id,
        workflow_id: current.request.workflow_id,
        workflow_revision: current.workflow_revision,
        origin: origin(&current.request.origin)?,
        deadline_at: include_deadline
            .then(|| current.deadline_at_ms.map(UnixTimestampMilliseconds::new))
            .flatten(),
        execution: crate::transcript_effect_request::exact_transcript_request(
            transaction,
            current,
            include_deadline,
        )?,
    })
}

fn origin(value: &str) -> Result<TranscriptWorkflowOrigin, StorageError> {
    match value {
        "user" => Ok(TranscriptWorkflowOrigin::User),
        "automatic" => Ok(TranscriptWorkflowOrigin::Automatic),
        "playback" => Ok(TranscriptWorkflowOrigin::Playback),
        _ => Err(StorageError::InvalidActivity),
    }
}

fn fingerprint(input: TranscriptSubmissionClaimInput) -> ContentDigest {
    let mut hash = Sha256::new();
    hash.update(b"pod0/transcript/submission-authorization/v1");
    hash.update(input.episode_id.into_bytes());
    hash.update(input.request_id.into_bytes());
    hash.update(input.attempt_id.into_bytes());
    hash.update(input.submission_fence_id.into_bytes());
    hash.update(input.cancellation_id.into_bytes());
    hash.update(input.issued_revision.value.to_be_bytes());
    ContentDigest::from_bytes(hash.finalize().into())
}

fn recovery_fingerprint(record: &crate::TranscriptWorkflowRecord) -> ContentDigest {
    let mut hash = Sha256::new();
    hash.update(b"pod0/transcript/provider-recovery-authorization/v1");
    hash.update(record.episode_id.into_bytes());
    hash.update(record.request.workflow_id.into_bytes());
    hash.update(record.workflow_revision.value.to_be_bytes());
    hash.update(
        record
            .external_operation_id
            .as_deref()
            .unwrap_or_default()
            .as_bytes(),
    );
    ContentDigest::from_bytes(hash.finalize().into())
}

fn publisher_fingerprint(record: &crate::TranscriptWorkflowRecord) -> ContentDigest {
    let mut hash = Sha256::new();
    hash.update(b"pod0/transcript/publisher-authorization/v1");
    hash.update(record.episode_id.into_bytes());
    hash.update(record.request.workflow_id.into_bytes());
    hash.update(record.workflow_revision.value.to_be_bytes());
    hash.update(
        record
            .request
            .publisher_transcript_url
            .as_deref()
            .unwrap_or_default()
            .as_bytes(),
    );
    ContentDigest::from_bytes(hash.finalize().into())
}
