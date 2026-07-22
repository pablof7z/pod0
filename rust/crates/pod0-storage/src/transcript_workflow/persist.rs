use rusqlite::{Transaction, named_params};

use super::model::TranscriptWorkflowRecord;
use super::support::i64_value;
use crate::StorageError;

pub(super) fn persist_workflow(
    transaction: &Transaction<'_>,
    record: &TranscriptWorkflowRecord,
) -> Result<(), StorageError> {
    transaction
        .execute(
            "INSERT INTO pod0_transcript_workflows(
             episode_id,workflow_id,stage,source_revision,origin,provider,model,remote_audio_url,
             local_audio_url,publisher_transcript_url,publisher_mime_hint,publisher_first,
             provider_fallback_enabled,workflow_revision,attempt,max_attempts,attempt_id,
             submission_fence_id,command_id,cancellation_id,request_id,issued_revision,
             deadline_at_ms,not_before_ms,submission_authorized_at_ms,external_operation_id,
             provider_status,completion_artifact_id,committed_artifact_id,
             committed_transcript_version_id,committed_content_digest,expected_selection_revision,
             resulting_selection_revision,evidence_input_version,failure_code,failure_detail,
             failure_retryable,may_have_submitted,source_generation,created_at_ms,updated_at_ms
             ) VALUES(
             :episode,:workflow,:stage,:source,:origin,:provider,:model,:remote,:local,
             :publisher_url,:publisher_mime,:publisher_first,:fallback,:revision,:attempt,
             :max_attempts,:attempt_id,:fence,:command,:cancellation,:request_id,:issued,
             :deadline,:not_before,:authorized,:external_id,:provider_status,:completion_artifact,
             :committed_artifact,:version_id,:content_digest,:expected_selection,
             :resulting_selection,:evidence_input,:failure_code,:failure_detail,
             :failure_retryable,:may_submit,:source_generation,:created,:updated
             ) ON CONFLICT(episode_id) DO UPDATE SET
             workflow_id=excluded.workflow_id,stage=excluded.stage,
             source_revision=excluded.source_revision,origin=excluded.origin,
             provider=excluded.provider,model=excluded.model,remote_audio_url=excluded.remote_audio_url,
             local_audio_url=excluded.local_audio_url,
             publisher_transcript_url=excluded.publisher_transcript_url,
             publisher_mime_hint=excluded.publisher_mime_hint,
             publisher_first=excluded.publisher_first,
             provider_fallback_enabled=excluded.provider_fallback_enabled,
             workflow_revision=excluded.workflow_revision,attempt=excluded.attempt,
             max_attempts=excluded.max_attempts,attempt_id=excluded.attempt_id,
             submission_fence_id=excluded.submission_fence_id,command_id=excluded.command_id,
             cancellation_id=excluded.cancellation_id,request_id=excluded.request_id,
             issued_revision=excluded.issued_revision,deadline_at_ms=excluded.deadline_at_ms,
             not_before_ms=excluded.not_before_ms,
             submission_authorized_at_ms=excluded.submission_authorized_at_ms,
             external_operation_id=excluded.external_operation_id,
             provider_status=excluded.provider_status,
             completion_artifact_id=excluded.completion_artifact_id,
             committed_artifact_id=excluded.committed_artifact_id,
             committed_transcript_version_id=excluded.committed_transcript_version_id,
             committed_content_digest=excluded.committed_content_digest,
             expected_selection_revision=excluded.expected_selection_revision,
             resulting_selection_revision=excluded.resulting_selection_revision,
             evidence_input_version=excluded.evidence_input_version,
             failure_code=excluded.failure_code,failure_detail=excluded.failure_detail,
             failure_retryable=excluded.failure_retryable,
             may_have_submitted=excluded.may_have_submitted,
             source_generation=excluded.source_generation,created_at_ms=excluded.created_at_ms,
             updated_at_ms=excluded.updated_at_ms",
            named_params! {
                ":episode": record.episode_id.into_bytes().as_slice(),
                ":workflow": record.request.workflow_id.into_bytes().as_slice(),
                ":stage": record.stage.wire(),
                ":source": record.request.source_revision,
                ":origin": record.request.origin,
                ":provider": record.request.provider,
                ":model": record.request.model,
                ":remote": record.request.remote_audio_url,
                ":local": record.request.local_audio_url,
                ":publisher_url": record.request.publisher_transcript_url,
                ":publisher_mime": record.request.publisher_mime_hint,
                ":publisher_first": i64::from(record.request.publisher_first),
                ":fallback": i64::from(record.request.provider_fallback_enabled),
                ":revision": i64_value(record.workflow_revision.value)?,
                ":attempt": i64::from(record.attempt),
                ":max_attempts": i64::from(record.max_attempts),
                ":attempt_id": record.attempt_id.map(|id| id.into_bytes().to_vec()),
                ":fence": record.submission_fence_id.map(|id| id.into_bytes().to_vec()),
                ":command": record.command_id.into_bytes().as_slice(),
                ":cancellation": record.cancellation_id.into_bytes().as_slice(),
                ":request_id": record.request_id.map(|id| id.into_bytes().to_vec()),
                ":issued": i64_value(record.issued_revision.value)?,
                ":deadline": record.deadline_at_ms,
                ":not_before": record.not_before_ms,
                ":authorized": record.submission_authorized_at_ms,
                ":external_id": record.external_operation_id,
                ":provider_status": record.provider_status,
                ":completion_artifact": record.completion_artifact_id.map(|id| id.into_bytes().to_vec()),
                ":committed_artifact": record.committed_artifact_id.map(|id| id.into_bytes().to_vec()),
                ":version_id": record.committed_transcript_version_id.map(|id| id.into_bytes().to_vec()),
                ":content_digest": record.committed_content_digest.map(|id| id.into_bytes().to_vec()),
                ":expected_selection": i64_value(record.expected_selection_revision.value)?,
                ":resulting_selection": record.resulting_selection_revision.map(|value| i64_value(value.value)).transpose()?,
                ":evidence_input": record.evidence_input_version,
                ":failure_code": record.failure_code,
                ":failure_detail": record.failure_detail,
                ":failure_retryable": i64::from(record.failure_retryable),
                ":may_submit": i64::from(record.may_have_submitted),
                ":source_generation": record.source_generation.map(i64_value).transpose()?,
                ":created": record.created_at_ms,
                ":updated": record.updated_at_ms,
            },
        )
        .map_err(|error| StorageError::sqlite("persist transcript workflow", error))?;
    Ok(())
}

pub(super) fn insert_prepared_attempt(
    transaction: &Transaction<'_>,
    record: &TranscriptWorkflowRecord,
) -> Result<(), StorageError> {
    let (Some(attempt_id), Some(fence), Some(request_id)) = (
        record.attempt_id,
        record.submission_fence_id,
        record.request_id,
    ) else {
        return Ok(());
    };
    transaction
        .execute(
            "INSERT INTO pod0_transcript_attempts(attempt_id,workflow_id,episode_id,attempt,
             submission_fence_id,request_id,state,may_have_submitted,created_at_ms,updated_at_ms)
             VALUES(?1,?2,?3,?4,?5,?6,'prepared',0,?7,?7)",
            rusqlite::params![
                attempt_id.into_bytes().as_slice(),
                record.request.workflow_id.into_bytes().as_slice(),
                record.episode_id.into_bytes().as_slice(),
                i64::from(record.attempt),
                fence.into_bytes().as_slice(),
                request_id.into_bytes().as_slice(),
                record.updated_at_ms,
            ],
        )
        .map_err(|error| StorageError::sqlite("insert transcript attempt", error))?;
    Ok(())
}
