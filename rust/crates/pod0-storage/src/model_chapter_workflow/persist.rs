use rusqlite::{Transaction, named_params};

use super::model::ModelChapterWorkflowRecord;
use super::support::{artifact_source_code, i64_value};
use crate::StorageError;

pub(crate) fn persist_workflow(
    transaction: &Transaction<'_>,
    record: &ModelChapterWorkflowRecord,
) -> Result<(), StorageError> {
    let active = record.active_request.as_ref();
    let episode_id = record.episode_id.into_bytes().to_vec();
    let command_id = record.command_id.into_bytes().to_vec();
    let cancellation_id = record.cancellation_id.into_bytes().to_vec();
    let request_id = record.request_id.map(|value| value.into_bytes().to_vec());
    let fence_id = record
        .submission_fence_id
        .map(|value| value.into_bytes().to_vec());
    let fingerprint = active.map(|value| value.request_fingerprint.into_bytes().to_vec());
    let requested_version =
        active.map(|value| value.requested_transcript_version_id.into_bytes().to_vec());
    let requested_digest =
        active.map(|value| value.requested_transcript_digest.into_bytes().to_vec());
    let selected_version =
        active.map(|value| value.selected_transcript_version_id.into_bytes().to_vec());
    let selected_digest =
        active.map(|value| value.selected_transcript_digest.into_bytes().to_vec());
    let base_artifact_id = active
        .and_then(|value| value.base_artifact_id)
        .map(|value| value.into_bytes().to_vec());
    let base_digest = active
        .and_then(|value| value.base_integrity_digest)
        .map(|value| value.into_bytes().to_vec());
    let selected_artifact_id = record
        .selected_artifact_id
        .map(|value| value.into_bytes().to_vec());
    let expected_source = active
        .map(|value| artifact_source_code(value.expected_artifact_source))
        .transpose()?;
    transaction
        .execute(
            "INSERT INTO pod0_model_chapter_workflows(episode_id,state,\
             desired_configured_model,active_configured_model,replan_pending,mode,\
             source_version,request_fingerprint,generation,workflow_revision,attempt,\
             max_attempts,command_id,cancellation_id,request_id,submission_fence_id,\
             issued_revision,deadline_at_ms,not_before_ms,submission_authorized_at_ms,\
             requested_transcript_version_id,requested_transcript_digest,\
             selected_transcript_version_id,selected_transcript_digest,\
             expected_selection_revision,base_artifact_id,base_integrity_digest,\
             format_version,policy_version,provider,model,response_format_code,\
             maximum_completion_bytes,duration_ms,expected_artifact_source_code,\
             system_prompt,user_prompt,provider_operation_id,provider_status,\
             selected_artifact_id,failure_code,failure_detail,may_have_submitted,\
             created_at_ms,updated_at_ms) VALUES(:episode_id,:state,:desired_model,\
             :active_model,:replan,:mode,:source_version,:fingerprint,:generation,\
             :workflow_revision,:attempt,:max_attempts,:command_id,:cancellation_id,\
             :request_id,:fence_id,:issued_revision,:deadline,:not_before,\
             :authorized_at,:requested_version,:requested_digest,:selected_version,\
             :selected_digest,:selection_revision,:base_artifact_id,:base_digest,\
             :format_version,:policy_version,:provider,:model,:response_format,\
             :maximum_bytes,:duration_ms,:expected_source,:system_prompt,:user_prompt,\
             :provider_operation_id,:provider_status,:selected_artifact_id,:failure_code,\
             :failure_detail,:may_have_submitted,:created_at,:updated_at) \
             ON CONFLICT(episode_id) DO UPDATE SET state=excluded.state,\
             desired_configured_model=excluded.desired_configured_model,\
             active_configured_model=excluded.active_configured_model,\
             replan_pending=excluded.replan_pending,mode=excluded.mode,\
             source_version=excluded.source_version,request_fingerprint=excluded.request_fingerprint,\
             generation=excluded.generation,workflow_revision=excluded.workflow_revision,\
             attempt=excluded.attempt,max_attempts=excluded.max_attempts,\
             command_id=excluded.command_id,cancellation_id=excluded.cancellation_id,\
             request_id=excluded.request_id,submission_fence_id=excluded.submission_fence_id,\
             issued_revision=excluded.issued_revision,deadline_at_ms=excluded.deadline_at_ms,\
             not_before_ms=excluded.not_before_ms,\
             submission_authorized_at_ms=excluded.submission_authorized_at_ms,\
             requested_transcript_version_id=excluded.requested_transcript_version_id,\
             requested_transcript_digest=excluded.requested_transcript_digest,\
             selected_transcript_version_id=excluded.selected_transcript_version_id,\
             selected_transcript_digest=excluded.selected_transcript_digest,\
             expected_selection_revision=excluded.expected_selection_revision,\
             base_artifact_id=excluded.base_artifact_id,\
             base_integrity_digest=excluded.base_integrity_digest,\
             format_version=excluded.format_version,policy_version=excluded.policy_version,\
             provider=excluded.provider,model=excluded.model,\
             response_format_code=excluded.response_format_code,\
             maximum_completion_bytes=excluded.maximum_completion_bytes,\
             duration_ms=excluded.duration_ms,\
             expected_artifact_source_code=excluded.expected_artifact_source_code,\
             system_prompt=excluded.system_prompt,user_prompt=excluded.user_prompt,\
             provider_operation_id=excluded.provider_operation_id,\
             provider_status=excluded.provider_status,\
             selected_artifact_id=excluded.selected_artifact_id,\
             failure_code=excluded.failure_code,failure_detail=excluded.failure_detail,\
             may_have_submitted=excluded.may_have_submitted,\
             created_at_ms=excluded.created_at_ms,updated_at_ms=excluded.updated_at_ms",
            named_params! {
                ":episode_id": episode_id,
                ":state": record.state.wire(),
                ":desired_model": record.desired_configured_model,
                ":active_model": active.map(|value| value.configured_model.as_str()),
                ":replan": record.replan_pending,
                ":mode": active.map(|value| value.mode.wire()),
                ":source_version": active.map(|value| value.source_version.as_str()),
                ":fingerprint": fingerprint,
                ":generation": i64_value(record.generation)?,
                ":workflow_revision": i64_value(record.workflow_revision.value)?,
                ":attempt": i64::from(record.attempt),
                ":max_attempts": i64::from(record.max_attempts),
                ":command_id": command_id,
                ":cancellation_id": cancellation_id,
                ":request_id": request_id,
                ":fence_id": fence_id,
                ":issued_revision": i64_value(record.issued_revision.value)?,
                ":deadline": record.deadline_at_ms,
                ":not_before": record.not_before_ms,
                ":authorized_at": record.submission_authorized_at_ms,
                ":requested_version": requested_version,
                ":requested_digest": requested_digest,
                ":selected_version": selected_version,
                ":selected_digest": selected_digest,
                ":selection_revision": active.map(|value| i64_value(value.expected_selection_revision.value)).transpose()?,
                ":base_artifact_id": base_artifact_id,
                ":base_digest": base_digest,
                ":format_version": active.map(|value| i64::from(value.format_version)),
                ":policy_version": active.map(|value| i64::from(value.policy_version)),
                ":provider": active.map(|value| value.provider.as_str()),
                ":model": active.map(|value| value.model.as_str()),
                ":response_format": active.map(|value| i64::from(value.response_format_code)),
                ":maximum_bytes": active.map(|value| i64_value(value.maximum_completion_bytes)).transpose()?,
                ":duration_ms": active.and_then(|value| value.duration_ms).map(i64_value).transpose()?,
                ":expected_source": expected_source,
                ":system_prompt": active.map(|value| value.system_prompt.as_str()),
                ":user_prompt": active.map(|value| value.user_prompt.as_str()),
                ":provider_operation_id": record.provider_operation_id,
                ":provider_status": record.provider_status,
                ":selected_artifact_id": selected_artifact_id,
                ":failure_code": record.failure_code,
                ":failure_detail": record.failure_detail,
                ":may_have_submitted": record.may_have_submitted,
                ":created_at": record.created_at_ms,
                ":updated_at": record.updated_at_ms,
            },
        )
        .map_err(|error| StorageError::sqlite("persist model chapter workflow", error))?;
    Ok(())
}
