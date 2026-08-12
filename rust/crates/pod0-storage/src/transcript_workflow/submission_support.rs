fn validate_provider_acceptance(
    input: &TranscriptProviderAcceptedInput,
) -> Result<(), StorageError> {
    validate_time(input.observed_at_ms)?;
    if input.external_operation_id.is_empty()
        || input.external_operation_id.len() > 1_024
        || input
            .provider_status
            .as_ref()
            .is_some_and(|value| value.len() > 1_024)
    {
        return Err(StorageError::TranscriptWorkflowConflict);
    }
    Ok(())
}

#[cfg(test)]
fn replayed_provider_acceptance(
    current: TranscriptWorkflowRecord,
    input: &TranscriptProviderAcceptedInput,
) -> Result<TranscriptWorkflowRecord, StorageError> {
    if current.external_operation_id.as_deref() == Some(&input.external_operation_id)
        && current.provider_status == input.provider_status
    {
        Ok(current)
    } else {
        Err(StorageError::TranscriptWorkflowConflict)
    }
}

#[cfg(test)]
fn update_provider_acceptance(
    transaction: &rusqlite::Transaction<'_>,
    input: &TranscriptProviderAcceptedInput,
) -> Result<(), StorageError> {
    for sql in [
        "UPDATE pod0_transcript_workflows SET stage='provider_accepted',workflow_revision=workflow_revision+1,
         external_operation_id=?1,provider_status=?2,updated_at_ms=?3 WHERE episode_id=?4 AND request_id=?5
         AND attempt_id=?6 AND submission_fence_id=?7 AND stage='submission_authorized'",
        "UPDATE pod0_transcript_attempts SET state='provider_accepted',external_operation_id=?1,
         provider_status=?2,updated_at_ms=?3 WHERE episode_id=?4 AND request_id=?5 AND attempt_id=?6
         AND submission_fence_id=?7 AND state='authorized'",
    ] {
        transaction.execute(sql,params![input.external_operation_id,input.provider_status,input.observed_at_ms,
            input.episode_id.into_bytes().as_slice(),input.request_id.into_bytes().as_slice(),
            input.attempt_id.into_bytes().as_slice(),input.submission_fence_id.into_bytes().as_slice()])
            .map_err(|error| StorageError::sqlite("record transcript provider acceptance", error))?;
        require_one_change(transaction)?;
    }
    Ok(())
}

fn require_one_change(transaction: &rusqlite::Transaction<'_>) -> Result<(), StorageError> {
    if transaction.changes() == 1 {
        Ok(())
    } else {
        Err(StorageError::StaleTranscriptAttempt)
    }
}
