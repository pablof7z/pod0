use rusqlite::params;

use super::authority::require_authoritative;
use super::model::*;
use super::read::read_workflow;
use super::support::validate_time;
use crate::{LibraryStore, StorageError};

impl LibraryStore {
    pub fn authorize_transcript_publisher_effect(
        &self,
        episode_id: pod0_domain::EpisodeId,
        request_id: pod0_domain::HostRequestId,
        now_ms: i64,
    ) -> Result<TranscriptWorkflowRecord, StorageError> {
        validate_time(now_ms)?;
        crate::transition_commit::commit_transcript_publisher_effect(
            self.path(),
            episode_id,
            request_id,
            now_ms,
        )
    }

    pub fn authorize_transcript_recovery_effect(
        &self,
        episode_id: pod0_domain::EpisodeId,
        request_id: pod0_domain::HostRequestId,
        now_ms: i64,
    ) -> Result<TranscriptWorkflowRecord, StorageError> {
        validate_time(now_ms)?;
        crate::transition_commit::commit_transcript_recovery_effect(
            self.path(),
            episode_id,
            request_id,
            now_ms,
        )
    }

    pub fn claim_transcript_submission(
        &self,
        input: TranscriptSubmissionClaimInput,
    ) -> Result<TranscriptSubmissionClaim, StorageError> {
        validate_time(input.now_ms)?;
        crate::transition_commit::commit_transcript_submission(self.path(), input)
    }

    #[cfg(test)]
    pub(crate) fn claim_transcript_submission_with_observer<F>(
        &self,
        input: TranscriptSubmissionClaimInput,
        before_commit: F,
    ) -> Result<TranscriptSubmissionClaim, StorageError>
    where
        F: FnOnce() -> Result<(), StorageError>,
    {
        validate_time(input.now_ms)?;
        self.write(|transaction| {
            require_authoritative(transaction)?;
            let record = exact_attempt(
                transaction,
                input.episode_id,
                input.request_id,
                input.attempt_id,
                input.submission_fence_id,
            )?;
            if record.stage.protects_submission() {
                return Ok(TranscriptSubmissionClaim::AlreadyClaimed(record));
            }
            validate_claim(&record, &input)?;
            authorize_submission(transaction, &input)?;
            before_commit()?;
            Ok(TranscriptSubmissionClaim::Authorized(
                read_workflow(transaction, input.episode_id)?
                    .ok_or(StorageError::TranscriptWorkflowNotFound)?,
            ))
        })
    }

    #[cfg(test)]
    pub(crate) fn record_transcript_provider_accepted(
        &self,
        input: TranscriptProviderAcceptedInput,
    ) -> Result<TranscriptWorkflowRecord, StorageError> {
        self.record_transcript_provider_accepted_with_observer(input, || Ok(()))
    }

    #[cfg(test)]
    pub(crate) fn record_transcript_provider_accepted_with_observer<F>(
        &self,
        input: TranscriptProviderAcceptedInput,
        before_commit: F,
    ) -> Result<TranscriptWorkflowRecord, StorageError>
    where
        F: FnOnce() -> Result<(), StorageError>,
    {
        validate_provider_acceptance(&input)?;
        self.write(|transaction| {
            require_authoritative(transaction)?;
            let current = exact_attempt(
                transaction,
                input.episode_id,
                input.request_id,
                input.attempt_id,
                input.submission_fence_id,
            )?;
            if current.stage == StoredTranscriptWorkflowStage::ProviderAccepted {
                return replayed_provider_acceptance(current, &input);
            }
            if current.stage != StoredTranscriptWorkflowStage::SubmissionAuthorized
                || input.observed_at_ms < current.updated_at_ms
            {
                return Err(StorageError::StaleTranscriptAttempt);
            }
            update_provider_acceptance(transaction, &input)?;
            before_commit()?;
            read_workflow(transaction, input.episode_id)?
                .ok_or(StorageError::TranscriptWorkflowNotFound)
        })
    }
}

pub(crate) fn apply_transcript_provider_accepted(
    transaction: &rusqlite::Transaction<'_>,
    input: TranscriptProviderAcceptedInput,
    not_before_ms: i64,
) -> Result<TranscriptWorkflowRecord, StorageError> {
    validate_provider_acceptance(&input)?;
    validate_time(not_before_ms)?;
    if not_before_ms < input.observed_at_ms {
        return Err(StorageError::TranscriptWorkflowConflict);
    }
    require_authoritative(transaction)?;
    let mut record = exact_attempt(
        transaction,
        input.episode_id,
        input.request_id,
        input.attempt_id,
        input.submission_fence_id,
    )?;
    if record.stage != StoredTranscriptWorkflowStage::SubmissionAuthorized
        || input.observed_at_ms < record.updated_at_ms
    {
        return Err(StorageError::StaleTranscriptAttempt);
    }
    record.stage = StoredTranscriptWorkflowStage::ProviderAccepted;
    record.workflow_revision = super::support::next_revision(record.workflow_revision)?;
    record.external_operation_id = Some(input.external_operation_id);
    record.provider_status = input.provider_status;
    record.not_before_ms = Some(not_before_ms);
    record.updated_at_ms = input.observed_at_ms;
    super::persist::persist_workflow(transaction, &record)?;
    let changed = transaction
        .execute(
            "UPDATE pod0_transcript_attempts SET state='provider_accepted',external_operation_id=?1,
             provider_status=?2,updated_at_ms=?3 WHERE episode_id=?4 AND request_id=?5
             AND attempt_id=?6 AND submission_fence_id=?7 AND state='authorized'",
            params![
                record.external_operation_id,
                record.provider_status,
                input.observed_at_ms,
                input.episode_id.into_bytes().as_slice(),
                input.request_id.into_bytes().as_slice(),
                input.attempt_id.into_bytes().as_slice(),
                input.submission_fence_id.into_bytes().as_slice()
            ],
        )
        .map_err(|error| StorageError::sqlite("record transcript provider acceptance", error))?;
    if changed != 1 {
        return Err(StorageError::StaleTranscriptAttempt);
    }
    Ok(record)
}

pub(crate) fn validate_claim(
    record: &TranscriptWorkflowRecord,
    input: &TranscriptSubmissionClaimInput,
) -> Result<(), StorageError> {
    if !matches!(
        record.stage,
        StoredTranscriptWorkflowStage::Requested | StoredTranscriptWorkflowStage::RetryScheduled
    ) || record.cancellation_id != input.cancellation_id
        || record.issued_revision != input.issued_revision
        || record
            .not_before_ms
            .is_some_and(|value| value > input.now_ms)
        || record
            .deadline_at_ms
            .is_none_or(|value| value < input.now_ms)
    {
        return Err(StorageError::TranscriptWorkflowConflict);
    }
    Ok(())
}

pub(crate) fn authorize_submission(
    transaction: &rusqlite::Transaction<'_>,
    input: &TranscriptSubmissionClaimInput,
) -> Result<(), StorageError> {
    transaction
        .execute(
            "UPDATE pod0_transcript_workflows SET stage='submission_authorized',
             workflow_revision=workflow_revision+1,not_before_ms=NULL,
             submission_authorized_at_ms=?1,may_have_submitted=1,updated_at_ms=?1
             WHERE episode_id=?2 AND request_id=?3 AND attempt_id=?4
             AND submission_fence_id=?5 AND stage IN('requested','retry_scheduled')",
            params![
                input.now_ms,
                input.episode_id.into_bytes().as_slice(),
                input.request_id.into_bytes().as_slice(),
                input.attempt_id.into_bytes().as_slice(),
                input.submission_fence_id.into_bytes().as_slice()
            ],
        )
        .map_err(|error| StorageError::sqlite("authorize transcript submission", error))?;
    require_one_change(transaction)?;
    transaction
        .execute(
            "UPDATE pod0_transcript_attempts SET state='authorized',authorized_at_ms=?1,
             may_have_submitted=1,updated_at_ms=?1 WHERE attempt_id=?2 AND request_id=?3
             AND submission_fence_id=?4 AND state='prepared'",
            params![
                input.now_ms,
                input.attempt_id.into_bytes().as_slice(),
                input.request_id.into_bytes().as_slice(),
                input.submission_fence_id.into_bytes().as_slice()
            ],
        )
        .map_err(|error| StorageError::sqlite("authorize transcript attempt", error))?;
    require_one_change(transaction)
}

pub(crate) fn exact_attempt(
    transaction: &rusqlite::Transaction<'_>,
    episode_id: pod0_domain::EpisodeId,
    request_id: pod0_domain::HostRequestId,
    attempt_id: pod0_domain::TranscriptAttemptId,
    fence: pod0_domain::TranscriptSubmissionFenceId,
) -> Result<TranscriptWorkflowRecord, StorageError> {
    let record =
        read_workflow(transaction, episode_id)?.ok_or(StorageError::TranscriptWorkflowNotFound)?;
    if record.request_id != Some(request_id)
        || record.attempt_id != Some(attempt_id)
        || record.submission_fence_id != Some(fence)
    {
        return Err(StorageError::StaleTranscriptAttempt);
    }
    Ok(record)
}

include!("submission_support.rs");
