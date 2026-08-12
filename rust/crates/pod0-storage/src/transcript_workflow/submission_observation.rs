use rusqlite::params;

use super::authority::require_authoritative;
use super::model::{
    StoredTranscriptWorkflowStage, TranscriptProviderPendingInput, TranscriptWorkflowRecord,
};
use super::persist::persist_workflow;
use super::submission::exact_attempt;
use super::support::{next_revision, validate_time};
use crate::StorageError;

pub(crate) fn apply_transcript_provider_pending(
    transaction: &rusqlite::Transaction<'_>,
    input: TranscriptProviderPendingInput,
) -> Result<TranscriptWorkflowRecord, StorageError> {
    validate_time(input.observed_at_ms)?;
    validate_time(input.not_before_ms)?;
    if input.not_before_ms < input.observed_at_ms
        || input
            .provider_status
            .as_ref()
            .is_some_and(|value| value.len() > 1_024)
    {
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
    if record.stage != StoredTranscriptWorkflowStage::ProviderAccepted
        || input.observed_at_ms < record.updated_at_ms
    {
        return Err(StorageError::StaleTranscriptAttempt);
    }
    record.workflow_revision = next_revision(record.workflow_revision)?;
    record.provider_status = input.provider_status;
    record.not_before_ms = Some(input.not_before_ms);
    record.updated_at_ms = input.observed_at_ms;
    persist_workflow(transaction, &record)?;
    let changed = transaction
        .execute(
            "UPDATE pod0_transcript_attempts SET provider_status=?1,updated_at_ms=?2
             WHERE attempt_id=?3 AND state='provider_accepted'",
            params![
                record.provider_status,
                input.observed_at_ms,
                input.attempt_id.into_bytes().as_slice()
            ],
        )
        .map_err(|error| StorageError::sqlite("record transcript provider pending", error))?;
    if changed != 1 {
        return Err(StorageError::StaleTranscriptAttempt);
    }
    Ok(record)
}
