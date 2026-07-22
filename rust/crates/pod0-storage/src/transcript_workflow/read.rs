use pod0_domain::{
    CancellationId, CommandId, EpisodeId, HostRequestId, StateRevision, TranscriptArtifactId,
    TranscriptAttemptId, TranscriptSubmissionFenceId, TranscriptVersionId, TranscriptWorkflowId,
};
use rusqlite::{Connection, OptionalExtension, Row};

use super::model::{
    MAX_TRANSCRIPT_WORKFLOW_PAGE_ITEMS, StoredTranscriptWorkflowRequest,
    StoredTranscriptWorkflowStage, TranscriptWorkflowPage, TranscriptWorkflowRecord,
};
use super::support::{bool_value, bytes16, optional_digest, optional_id, unsigned, unsigned16};
use crate::StorageError;

pub(super) const WORKFLOW_COLUMNS: &str = "episode_id,workflow_id,stage,source_revision,origin,provider,model,remote_audio_url,local_audio_url,publisher_transcript_url,publisher_mime_hint,publisher_first,provider_fallback_enabled,workflow_revision,attempt,max_attempts,attempt_id,submission_fence_id,command_id,cancellation_id,request_id,issued_revision,deadline_at_ms,not_before_ms,submission_authorized_at_ms,external_operation_id,provider_status,completion_artifact_id,committed_artifact_id,committed_transcript_version_id,committed_content_digest,expected_selection_revision,resulting_selection_revision,evidence_input_version,failure_code,failure_detail,failure_retryable,may_have_submitted,source_generation,created_at_ms,updated_at_ms";

pub(super) fn read_workflow(
    connection: &Connection,
    episode_id: EpisodeId,
) -> Result<Option<TranscriptWorkflowRecord>, StorageError> {
    connection
        .query_row(
            &format!(
                "SELECT {WORKFLOW_COLUMNS} FROM pod0_transcript_workflows WHERE episode_id=?1"
            ),
            [episode_id.into_bytes().as_slice()],
            decode_row,
        )
        .optional()
        .map_err(|_| StorageError::TranscriptWorkflowConflict)
}

pub(super) fn read_page(
    connection: &Connection,
    offset: u32,
    maximum_count: u16,
) -> Result<TranscriptWorkflowPage, StorageError> {
    let limit = maximum_count.clamp(1, MAX_TRANSCRIPT_WORKFLOW_PAGE_ITEMS);
    let requested = i64::from(limit) + 1;
    let mut statement = connection
        .prepare(&format!(
            "SELECT {WORKFLOW_COLUMNS} FROM pod0_transcript_workflows \
             ORDER BY updated_at_ms DESC,episode_id LIMIT ?1 OFFSET ?2"
        ))
        .map_err(|error| StorageError::sqlite("prepare transcript workflow page", error))?;
    let rows = statement
        .query_map([requested, i64::from(offset)], decode_row)
        .map_err(|error| StorageError::sqlite("query transcript workflow page", error))?;
    let mut items = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| StorageError::TranscriptWorkflowConflict)?;
    let has_more = items.len() > usize::from(limit);
    items.truncate(usize::from(limit));
    Ok(TranscriptWorkflowPage { items, has_more })
}

pub(super) fn decode_row(row: &Row<'_>) -> rusqlite::Result<TranscriptWorkflowRecord> {
    decode_row_inner(row).map_err(|_| rusqlite::Error::InvalidQuery)
}

fn decode_row_inner(row: &Row<'_>) -> Result<TranscriptWorkflowRecord, StorageError> {
    let stage = StoredTranscriptWorkflowStage::parse(&row.get::<_, String>(2)?)
        .ok_or(StorageError::TranscriptWorkflowConflict)?;
    Ok(TranscriptWorkflowRecord {
        episode_id: EpisodeId::from_bytes(bytes16(row.get(0)?)?),
        request: StoredTranscriptWorkflowRequest {
            workflow_id: TranscriptWorkflowId::from_bytes(bytes16(row.get(1)?)?),
            source_revision: row.get(3)?,
            origin: row.get(4)?,
            provider: row.get(5)?,
            model: row.get(6)?,
            remote_audio_url: row.get(7)?,
            local_audio_url: row.get(8)?,
            publisher_transcript_url: row.get(9)?,
            publisher_mime_hint: row.get(10)?,
            publisher_first: bool_value(row.get(11)?)?,
            provider_fallback_enabled: bool_value(row.get(12)?)?,
        },
        stage,
        workflow_revision: StateRevision::new(unsigned(row.get(13)?)?),
        attempt: unsigned16(row.get(14)?)?,
        max_attempts: unsigned16(row.get(15)?)?,
        attempt_id: optional_id(row.get(16)?, TranscriptAttemptId::from_bytes)?,
        submission_fence_id: optional_id(row.get(17)?, TranscriptSubmissionFenceId::from_bytes)?,
        command_id: CommandId::from_bytes(bytes16(row.get(18)?)?),
        cancellation_id: CancellationId::from_bytes(bytes16(row.get(19)?)?),
        request_id: optional_id(row.get(20)?, HostRequestId::from_bytes)?,
        issued_revision: StateRevision::new(unsigned(row.get(21)?)?),
        deadline_at_ms: row.get(22)?,
        not_before_ms: row.get(23)?,
        submission_authorized_at_ms: row.get(24)?,
        external_operation_id: row.get(25)?,
        provider_status: row.get(26)?,
        completion_artifact_id: optional_id(row.get(27)?, TranscriptArtifactId::from_bytes)?,
        committed_artifact_id: optional_id(row.get(28)?, TranscriptArtifactId::from_bytes)?,
        committed_transcript_version_id: optional_id(
            row.get(29)?,
            TranscriptVersionId::from_bytes,
        )?,
        committed_content_digest: optional_digest(row.get(30)?)?,
        expected_selection_revision: StateRevision::new(unsigned(row.get(31)?)?),
        resulting_selection_revision: row
            .get::<_, Option<i64>>(32)?
            .map(unsigned)
            .transpose()?
            .map(StateRevision::new),
        evidence_input_version: row.get(33)?,
        failure_code: row.get(34)?,
        failure_detail: row.get(35)?,
        failure_retryable: bool_value(row.get(36)?)?,
        may_have_submitted: bool_value(row.get(37)?)?,
        source_generation: row.get::<_, Option<i64>>(38)?.map(unsigned).transpose()?,
        created_at_ms: row.get(39)?,
        updated_at_ms: row.get(40)?,
    })
}
