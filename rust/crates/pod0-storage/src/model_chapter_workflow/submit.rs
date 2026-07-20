use pod0_domain::ContentDigest;
use rusqlite::params;
use sha2::{Digest as _, Sha256};

use super::inputs::{
    ModelChapterCompletionInput, ModelChapterCompletionRecord, ModelChapterProviderAcceptedInput,
    ModelChapterSubmissionClaim, ModelChapterSubmissionClaimInput,
};
use super::model::{ModelChapterWorkflowRecord, ModelChapterWorkflowState};
use super::read::{read_completion, read_workflow};
use super::support::i64_value;
use crate::{LibraryStore, StorageError};

impl LibraryStore {
    pub fn claim_model_chapter_submission(
        &self,
        input: ModelChapterSubmissionClaimInput,
    ) -> Result<ModelChapterSubmissionClaim, StorageError> {
        self.write(|transaction| {
            let mut record = exact_claim_record(transaction, &input)?;
            if record.state.may_have_submitted() {
                return Ok(ModelChapterSubmissionClaim::AlreadyClaimed(record));
            }
            if !matches!(
                record.state,
                ModelChapterWorkflowState::Requested | ModelChapterWorkflowState::RetryScheduled
            ) || record
                .not_before_ms
                .is_some_and(|value| value > input.now_ms)
                || record
                    .deadline_at_ms
                    .is_none_or(|value| value < input.now_ms)
                || input.now_ms < 0
            {
                return Err(StorageError::ChapterWorkflowConflict);
            }
            transaction
                .execute(
                    "UPDATE pod0_model_chapter_workflows SET state='submission_authorized',\
                     workflow_revision=workflow_revision+1,not_before_ms=NULL,\
                     submission_authorized_at_ms=?1,may_have_submitted=1,updated_at_ms=?1 \
                     WHERE episode_id=?2 AND request_id=?3 AND generation=?4 \
                     AND cancellation_id=?5 AND issued_revision=?6 \
                     AND state IN('requested','retry_scheduled')",
                    params![
                        input.now_ms,
                        input.episode_id.into_bytes().as_slice(),
                        input.request_id.into_bytes().as_slice(),
                        i64_value(input.generation)?,
                        input.cancellation_id.into_bytes().as_slice(),
                        i64_value(input.issued_revision.value)?,
                    ],
                )
                .map_err(|error| StorageError::sqlite("claim model chapter submission", error))?;
            if transaction.changes() != 1 {
                return Err(StorageError::ChapterWorkflowConflict);
            }
            record = read_workflow(transaction, input.episode_id)?
                .ok_or(StorageError::ChapterWorkflowNotFound)?;
            Ok(ModelChapterSubmissionClaim::Authorized(record))
        })
    }

    pub fn record_model_chapter_provider_accepted(
        &self,
        input: ModelChapterProviderAcceptedInput,
    ) -> Result<ModelChapterWorkflowRecord, StorageError> {
        self.write(|transaction| {
            let current = exact_submission_record(
                transaction,
                input.episode_id,
                input.request_id,
                input.generation,
                input.submission_fence_id,
            )?;
            if input.observed_at_ms < 0
                || input.provider_operation_id.is_empty()
                || input.provider_operation_id.len() > 1_024
                || input
                    .provider_status
                    .as_ref()
                    .is_some_and(|value| value.len() > 1_024)
            {
                return Err(StorageError::ChapterWorkflowConflict);
            }
            if current.state == ModelChapterWorkflowState::ProviderAccepted
                && current.provider_operation_id.as_deref()
                    == Some(input.provider_operation_id.as_str())
                && current.provider_status == input.provider_status
            {
                return Ok(current);
            }
            if current.state != ModelChapterWorkflowState::SubmissionAuthorized {
                return Err(StorageError::ChapterWorkflowConflict);
            }
            transaction
                .execute(
                    "UPDATE pod0_model_chapter_workflows SET state='provider_accepted',\
                     workflow_revision=workflow_revision+1,provider_operation_id=?1,\
                     provider_status=?2,updated_at_ms=?3 WHERE episode_id=?4 AND request_id=?5 \
                     AND generation=?6 AND submission_fence_id=?7 \
                     AND state='submission_authorized'",
                    params![
                        input.provider_operation_id,
                        input.provider_status,
                        input.observed_at_ms,
                        input.episode_id.into_bytes().as_slice(),
                        input.request_id.into_bytes().as_slice(),
                        i64_value(input.generation)?,
                        input.submission_fence_id.into_bytes().as_slice(),
                    ],
                )
                .map_err(|error| {
                    StorageError::sqlite("accept model chapter provider job", error)
                })?;
            if transaction.changes() != 1 {
                return Err(StorageError::ChapterWorkflowConflict);
            }
            read_workflow(transaction, input.episode_id)?
                .ok_or(StorageError::ChapterWorkflowNotFound)
        })
    }

    pub fn stage_model_chapter_completion(
        &self,
        input: ModelChapterCompletionInput,
    ) -> Result<ModelChapterCompletionRecord, StorageError> {
        self.write(|transaction| {
            let workflow = exact_submission_record(
                transaction,
                input.episode_id,
                input.request_id,
                input.generation,
                input.submission_fence_id,
            )?;
            let active = workflow
                .active_request
                .as_ref()
                .ok_or(StorageError::ChapterWorkflowConflict)?;
            validate_completion(&input, active)?;
            let completion = completion_record(input);
            if let Some(existing) = read_completion(transaction, completion.request_id)? {
                return if existing == completion
                    && workflow.state == ModelChapterWorkflowState::CompletionObserved
                {
                    Ok(existing)
                } else {
                    Err(StorageError::ChapterWorkflowConflict)
                };
            }
            if !matches!(
                workflow.state,
                ModelChapterWorkflowState::SubmissionAuthorized
                    | ModelChapterWorkflowState::ProviderAccepted
            ) {
                return Err(StorageError::ChapterWorkflowConflict);
            }
            insert_completion(transaction, &completion)?;
            transaction
                .execute(
                    "UPDATE pod0_model_chapter_workflows SET state='completion_observed',\
                     workflow_revision=workflow_revision+1,provider_operation_id=COALESCE(?1,\
                     provider_operation_id),provider_status=COALESCE(?2,provider_status),\
                     updated_at_ms=?3 WHERE episode_id=?4 AND request_id=?5 AND generation=?6 \
                     AND submission_fence_id=?7 \
                     AND state IN('submission_authorized','provider_accepted')",
                    params![
                        completion.provider_operation_id,
                        completion.provider_status,
                        completion.observed_at_ms,
                        completion.episode_id.into_bytes().as_slice(),
                        completion.request_id.into_bytes().as_slice(),
                        i64_value(completion.generation)?,
                        completion.submission_fence_id.into_bytes().as_slice(),
                    ],
                )
                .map_err(|error| StorageError::sqlite("stage model chapter completion", error))?;
            if transaction.changes() != 1 {
                return Err(StorageError::ChapterWorkflowConflict);
            }
            Ok(completion)
        })
    }
}

fn exact_claim_record(
    transaction: &rusqlite::Transaction<'_>,
    input: &ModelChapterSubmissionClaimInput,
) -> Result<ModelChapterWorkflowRecord, StorageError> {
    let record = read_workflow(transaction, input.episode_id)?
        .ok_or(StorageError::ChapterWorkflowNotFound)?;
    if record.request_id != Some(input.request_id)
        || record.generation != input.generation
        || record.cancellation_id != input.cancellation_id
        || record.issued_revision != input.issued_revision
    {
        return Err(StorageError::ChapterWorkflowConflict);
    }
    Ok(record)
}

fn exact_submission_record(
    transaction: &rusqlite::Transaction<'_>,
    episode_id: pod0_domain::EpisodeId,
    request_id: pod0_domain::HostRequestId,
    generation: u64,
    fence: pod0_domain::ChapterModelSubmissionFenceId,
) -> Result<ModelChapterWorkflowRecord, StorageError> {
    let record =
        read_workflow(transaction, episode_id)?.ok_or(StorageError::ChapterWorkflowNotFound)?;
    if record.request_id != Some(request_id)
        || record.generation != generation
        || record.submission_fence_id != Some(fence)
    {
        return Err(StorageError::ChapterWorkflowConflict);
    }
    Ok(record)
}

fn validate_completion(
    input: &ModelChapterCompletionInput,
    request: &super::model::StoredModelChapterRequest,
) -> Result<(), StorageError> {
    if input.completion.is_empty()
        || input.completion.len() as u64 > request.maximum_completion_bytes
        || input.completion.len() > 1_048_576
        || input.provider != request.provider
        || input.model != request.model
        || input.generated_at_ms < 0
        || input.observed_at_ms < 0
    {
        return Err(StorageError::ChapterWorkflowConflict);
    }
    Ok(())
}

fn completion_record(input: ModelChapterCompletionInput) -> ModelChapterCompletionRecord {
    ModelChapterCompletionRecord {
        request_id: input.request_id,
        episode_id: input.episode_id,
        generation: input.generation,
        submission_fence_id: input.submission_fence_id,
        completion_digest: ContentDigest::from_bytes(
            Sha256::digest(input.completion.as_bytes()).into(),
        ),
        completion: input.completion,
        provider: input.provider,
        model: input.model,
        prompt_tokens: input.prompt_tokens,
        completion_tokens: input.completion_tokens,
        cached_tokens: input.cached_tokens,
        reasoning_tokens: input.reasoning_tokens,
        cost_microusd: input.cost_microusd,
        provider_operation_id: input.provider_operation_id,
        provider_status: input.provider_status,
        generated_at_ms: input.generated_at_ms,
        observed_at_ms: input.observed_at_ms,
    }
}

fn insert_completion(
    transaction: &rusqlite::Transaction<'_>,
    completion: &ModelChapterCompletionRecord,
) -> Result<(), StorageError> {
    transaction
        .execute(
            "INSERT INTO pod0_model_chapter_completions(request_id,episode_id,generation,\
         submission_fence_id,completion,completion_digest,provider,model,prompt_tokens,\
         completion_tokens,cached_tokens,reasoning_tokens,cost_microusd,provider_operation_id,\
         provider_status,generated_at_ms,observed_at_ms) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,\
         ?9,?10,?11,?12,?13,?14,?15,?16,?17)",
            params![
                completion.request_id.into_bytes().as_slice(),
                completion.episode_id.into_bytes().as_slice(),
                i64_value(completion.generation)?,
                completion.submission_fence_id.into_bytes().as_slice(),
                completion.completion,
                completion.completion_digest.into_bytes().as_slice(),
                completion.provider,
                completion.model,
                completion.prompt_tokens.map(i64_value).transpose()?,
                completion.completion_tokens.map(i64_value).transpose()?,
                completion.cached_tokens.map(i64_value).transpose()?,
                completion.reasoning_tokens.map(i64_value).transpose()?,
                completion.cost_microusd.map(i64_value).transpose()?,
                completion.provider_operation_id,
                completion.provider_status,
                completion.generated_at_ms,
                completion.observed_at_ms,
            ],
        )
        .map_err(|error| StorageError::sqlite("insert model chapter completion", error))?;
    Ok(())
}
