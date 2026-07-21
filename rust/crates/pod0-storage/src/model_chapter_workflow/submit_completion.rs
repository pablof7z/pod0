use pod0_domain::ContentDigest;
use rusqlite::params;
use sha2::{Digest as _, Sha256};

use super::inputs::{ModelChapterCompletionInput, ModelChapterCompletionRecord};
use super::model::StoredModelChapterRequest;
use super::support::i64_value;
use crate::StorageError;

pub(super) fn validate_completion_shape(
    input: &ModelChapterCompletionInput,
) -> Result<(), StorageError> {
    if input.completion.is_empty()
        || input.completion.len() > 1_048_576
        || input.provider.is_empty()
        || input.provider.len() > 128
        || input.model.is_empty()
        || input.model.len() > 256
        || input
            .provider_operation_id
            .as_ref()
            .is_some_and(|value| value.is_empty() || value.len() > 1_024)
        || input
            .provider_status
            .as_ref()
            .is_some_and(|value| value.len() > 1_024)
        || input.generated_at_ms < 0
        || input.observed_at_ms < 0
    {
        return Err(StorageError::ChapterWorkflowConflict);
    }
    Ok(())
}

pub(super) fn validate_completion(
    input: &ModelChapterCompletionRecord,
    request: &StoredModelChapterRequest,
) -> Result<(), StorageError> {
    if input.completion.is_empty()
        || input.completion.len() as u64 > request.maximum_completion_bytes
        || input.completion.len() > 1_048_576
        || input.provider != request.provider
        || input.generated_at_ms < 0
        || input.observed_at_ms < 0
    {
        return Err(StorageError::ChapterWorkflowConflict);
    }
    Ok(())
}

pub(super) fn completion_record(
    input: ModelChapterCompletionInput,
) -> ModelChapterCompletionRecord {
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

pub(super) fn completion_replays(
    existing: &ModelChapterCompletionRecord,
    observed: &ModelChapterCompletionRecord,
) -> bool {
    existing.request_id == observed.request_id
        && existing.episode_id == observed.episode_id
        && existing.generation == observed.generation
        && existing.submission_fence_id == observed.submission_fence_id
        && existing.completion == observed.completion
        && existing.completion_digest == observed.completion_digest
        && existing.provider == observed.provider
        && existing.model == observed.model
        && existing.prompt_tokens == observed.prompt_tokens
        && existing.completion_tokens == observed.completion_tokens
        && existing.cached_tokens == observed.cached_tokens
        && existing.reasoning_tokens == observed.reasoning_tokens
        && existing.cost_microusd == observed.cost_microusd
        && observed
            .provider_operation_id
            .as_ref()
            .is_none_or(|value| existing.provider_operation_id.as_ref() == Some(value))
        && observed
            .provider_status
            .as_ref()
            .is_none_or(|value| existing.provider_status.as_ref() == Some(value))
        && existing.generated_at_ms == observed.generated_at_ms
        && existing.observed_at_ms == observed.observed_at_ms
}

pub(super) fn insert_completion(
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
