use rusqlite::{Transaction, params};

use super::model::{StoredTranscriptWorkflowStage, TranscriptWorkflowRecord};
use crate::StorageError;

pub(super) fn adopt_attempt_state(
    transaction: &Transaction<'_>,
    record: &TranscriptWorkflowRecord,
) -> Result<(), StorageError> {
    let Some(attempt_id) = record.attempt_id else {
        return Ok(());
    };
    let state = match record.stage {
        StoredTranscriptWorkflowStage::Requested => "prepared",
        StoredTranscriptWorkflowStage::ProviderAccepted => "provider_accepted",
        StoredTranscriptWorkflowStage::Blocked if record.may_have_submitted => "ambiguous",
        StoredTranscriptWorkflowStage::Cancelled => "cancelled",
        StoredTranscriptWorkflowStage::TranscriptCommitted
        | StoredTranscriptWorkflowStage::EvidenceRequested
        | StoredTranscriptWorkflowStage::Succeeded => "committed",
        _ => "failed",
    };
    transaction
        .execute(
            "UPDATE pod0_transcript_attempts SET state=?1,authorized_at_ms=?2,
             external_operation_id=?3,provider_status=?4,completion_artifact_id=?5,
             failure_code=?6,failure_detail=?7,may_have_submitted=?8,updated_at_ms=?9
             WHERE attempt_id=?10",
            params![
                state,
                record.submission_authorized_at_ms,
                record.external_operation_id,
                record.provider_status,
                record
                    .completion_artifact_id
                    .map(|id| id.into_bytes().to_vec()),
                record.failure_code,
                record.failure_detail,
                i64::from(record.may_have_submitted),
                record.updated_at_ms,
                attempt_id.into_bytes().as_slice()
            ],
        )
        .map_err(|error| StorageError::sqlite("adopt transcript attempt state", error))?;
    Ok(())
}

pub(super) fn adopt_evidence_state(
    transaction: &Transaction<'_>,
    record: &TranscriptWorkflowRecord,
) -> Result<(), StorageError> {
    let Some(input) = record.evidence_input_version.as_deref() else {
        return Ok(());
    };
    let state = if record.stage == StoredTranscriptWorkflowStage::Succeeded {
        "completed"
    } else {
        "requested"
    };
    transaction
        .execute(
            "INSERT INTO pod0_transcript_evidence_requests(workflow_id,episode_id,
             transcript_version_id,content_digest,input_version,state,requested_at_ms,completed_at_ms)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
            params![
                record.request.workflow_id.into_bytes().as_slice(),
                record.episode_id.into_bytes().as_slice(),
                record
                    .committed_transcript_version_id
                    .map(|id| id.into_bytes().to_vec()),
                record
                    .committed_content_digest
                    .map(|id| id.into_bytes().to_vec()),
                input,
                state,
                record.updated_at_ms,
                (state == "completed").then_some(record.updated_at_ms)
            ],
        )
        .map_err(|error| StorageError::sqlite("adopt transcript evidence request", error))?;
    Ok(())
}
