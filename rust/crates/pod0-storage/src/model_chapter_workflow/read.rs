use pod0_domain::{
    CancellationId, ChapterArtifactId, ChapterArtifactSource, ChapterModelSubmissionFenceId,
    CommandId, ContentDigest, EpisodeId, HostRequestId, StateRevision, TranscriptVersionId,
};
use rusqlite::{Connection, OptionalExtension, Row, params};

use super::model::{
    ModelChapterWorkflowMode, ModelChapterWorkflowPage, ModelChapterWorkflowRecord,
    ModelChapterWorkflowState, StoredModelChapterRequest,
};
use crate::{LibraryStore, StorageError};

pub(crate) const WORKFLOW_COLUMNS: &str = "episode_id,state,desired_configured_model,active_configured_model,replan_pending,mode,source_version,request_fingerprint,generation,workflow_revision,attempt,max_attempts,command_id,cancellation_id,request_id,submission_fence_id,issued_revision,deadline_at_ms,not_before_ms,submission_authorized_at_ms,requested_transcript_version_id,requested_transcript_digest,selected_transcript_version_id,selected_transcript_digest,expected_selection_revision,base_artifact_id,base_integrity_digest,format_version,policy_version,provider,model,response_format_code,maximum_completion_bytes,duration_ms,expected_artifact_source_code,system_prompt,user_prompt,provider_operation_id,provider_status,selected_artifact_id,failure_code,failure_detail,may_have_submitted,created_at_ms,updated_at_ms";

impl LibraryStore {
    pub fn model_chapter_workflow(
        &self,
        episode_id: EpisodeId,
    ) -> Result<Option<ModelChapterWorkflowRecord>, StorageError> {
        self.read(|connection| read_workflow(connection, episode_id))
    }

    pub fn active_model_chapter_workflows(
        &self,
        max_items: u16,
    ) -> Result<Vec<ModelChapterWorkflowRecord>, StorageError> {
        self.read(|connection| read_active_workflows(connection, max_items))
    }

    pub fn dispatchable_model_chapter_workflows(
        &self,
        max_items: u16,
    ) -> Result<Vec<ModelChapterWorkflowRecord>, StorageError> {
        self.read(|connection| read_dispatchable_workflows(connection, max_items))
    }

    pub fn model_chapter_workflow_page(
        &self,
        episode_id: Option<EpisodeId>,
        offset: u32,
        max_items: u16,
    ) -> Result<ModelChapterWorkflowPage, StorageError> {
        self.read(|connection| read_workflow_page(connection, episode_id, offset, max_items))
    }
}

fn read_workflow_page(
    connection: &Connection,
    episode_id: Option<EpisodeId>,
    offset: u32,
    max_items: u16,
) -> Result<ModelChapterWorkflowPage, StorageError> {
    let limit = usize::from(max_items.max(1));
    let fetch = i64::try_from(limit + 1).expect("bounded model workflow page limit");
    let mut statement = connection
        .prepare(&format!(
            "SELECT {WORKFLOW_COLUMNS} FROM pod0_model_chapter_workflows \
             WHERE (?1 IS NULL OR episode_id=?1) \
             ORDER BY updated_at_ms DESC,episode_id LIMIT ?2 OFFSET ?3"
        ))
        .map_err(|error| StorageError::sqlite("prepare bounded model workflows", error))?;
    let rows = statement
        .query_map(
            params![
                episode_id.map(|id| id.into_bytes().to_vec()),
                fetch,
                i64::from(offset)
            ],
            workflow_row,
        )
        .map_err(|error| StorageError::sqlite("query bounded model workflows", error))?;
    let mut items = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| StorageError::sqlite("decode bounded model workflows", error))?;
    let has_more = items.len() > limit;
    items.truncate(limit);
    Ok(ModelChapterWorkflowPage { items, has_more })
}

pub(crate) fn read_workflow(
    connection: &Connection,
    episode_id: EpisodeId,
) -> Result<Option<ModelChapterWorkflowRecord>, StorageError> {
    connection
        .query_row(
            &format!(
                "SELECT {WORKFLOW_COLUMNS} FROM pod0_model_chapter_workflows WHERE episode_id=?1"
            ),
            [episode_id.into_bytes().as_slice()],
            workflow_row,
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read model chapter workflow", error))
}

pub(crate) fn read_active_workflows(
    connection: &Connection,
    max_items: u16,
) -> Result<Vec<ModelChapterWorkflowRecord>, StorageError> {
    let mut statement = connection
        .prepare(&format!(
            "SELECT {WORKFLOW_COLUMNS} FROM pod0_model_chapter_workflows \
             WHERE state IN('requested','submission_authorized','provider_accepted',\
             'completion_observed','retry_scheduled') ORDER BY not_before_ms,episode_id LIMIT ?1"
        ))
        .map_err(|error| StorageError::sqlite("prepare active model chapter workflows", error))?;
    let rows = statement
        .query_map([i64::from(max_items.max(1))], workflow_row)
        .map_err(|error| StorageError::sqlite("query active model chapter workflows", error))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| StorageError::sqlite("decode active model chapter workflows", error))
}

fn read_dispatchable_workflows(
    connection: &Connection,
    max_items: u16,
) -> Result<Vec<ModelChapterWorkflowRecord>, StorageError> {
    let mut statement = connection
        .prepare(&format!(
            "SELECT {WORKFLOW_COLUMNS} FROM pod0_model_chapter_workflows \
             WHERE state IN('requested','provider_accepted','retry_scheduled') \
             ORDER BY not_before_ms,episode_id LIMIT ?1"
        ))
        .map_err(|error| StorageError::sqlite("prepare dispatchable model workflows", error))?;
    let rows = statement
        .query_map([i64::from(max_items.max(1))], workflow_row)
        .map_err(|error| StorageError::sqlite("query dispatchable model workflows", error))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| StorageError::sqlite("decode dispatchable model workflows", error))
}

pub(crate) fn read_recoverable_workflows(
    connection: &Connection,
    max_items: u16,
) -> Result<Vec<ModelChapterWorkflowRecord>, StorageError> {
    let mut statement = connection
        .prepare(&format!(
            "SELECT {WORKFLOW_COLUMNS} FROM pod0_model_chapter_workflows \
             WHERE state IN('submission_authorized','provider_accepted','completion_observed') \
             ORDER BY updated_at_ms,episode_id LIMIT ?1"
        ))
        .map_err(|error| StorageError::sqlite("prepare recoverable model workflows", error))?;
    let rows = statement
        .query_map([i64::from(max_items.max(1))], workflow_row)
        .map_err(|error| StorageError::sqlite("query recoverable model workflows", error))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| StorageError::sqlite("decode recoverable model workflows", error))
}

fn workflow_row(row: &Row<'_>) -> rusqlite::Result<ModelChapterWorkflowRecord> {
    let state =
        ModelChapterWorkflowState::parse(&row.get::<_, String>(1)?).ok_or_else(invalid_row)?;
    let active_configured_model: Option<String> = row.get(3)?;
    let active_request = active_configured_model
        .map(|configured_model| decode_request(row, configured_model))
        .transpose()?;
    Ok(ModelChapterWorkflowRecord {
        episode_id: EpisodeId::from_bytes(bytes16(row.get(0)?)?),
        state,
        desired_configured_model: row.get(2)?,
        active_request,
        replan_pending: bool_value(row.get(4)?)?,
        generation: unsigned(row.get(8)?)?,
        workflow_revision: StateRevision::new(unsigned(row.get(9)?)?),
        attempt: unsigned16(row.get(10)?)?,
        max_attempts: unsigned16(row.get(11)?)?,
        command_id: CommandId::from_bytes(bytes16(row.get(12)?)?),
        cancellation_id: CancellationId::from_bytes(bytes16(row.get(13)?)?),
        request_id: optional_id(row.get(14)?, HostRequestId::from_bytes)?,
        submission_fence_id: optional_id(row.get(15)?, ChapterModelSubmissionFenceId::from_bytes)?,
        issued_revision: StateRevision::new(unsigned(row.get(16)?)?),
        deadline_at_ms: row.get(17)?,
        not_before_ms: row.get(18)?,
        submission_authorized_at_ms: row.get(19)?,
        provider_operation_id: row.get(37)?,
        provider_status: row.get(38)?,
        selected_artifact_id: optional_id(row.get(39)?, ChapterArtifactId::from_bytes)?,
        failure_code: row.get(40)?,
        failure_detail: row.get(41)?,
        may_have_submitted: bool_value(row.get(42)?)?,
        created_at_ms: row.get(43)?,
        updated_at_ms: row.get(44)?,
    })
}

fn decode_request(
    row: &Row<'_>,
    configured_model: String,
) -> rusqlite::Result<StoredModelChapterRequest> {
    let mode = ModelChapterWorkflowMode::parse(&required(row.get::<_, Option<String>>(5)?)?)
        .ok_or_else(invalid_row)?;
    let base_artifact_id = optional_id(row.get(25)?, ChapterArtifactId::from_bytes)?;
    let base_integrity_digest = optional_digest(row.get(26)?)?;
    if (mode == ModelChapterWorkflowMode::Enrich)
        != (base_artifact_id.is_some() && base_integrity_digest.is_some())
    {
        return Err(invalid_row());
    }
    Ok(StoredModelChapterRequest {
        configured_model,
        mode,
        source_version: required(row.get(6)?)?,
        request_fingerprint: digest(required(row.get(7)?)?)?,
        requested_transcript_version_id: TranscriptVersionId::from_bytes(bytes16(required(
            row.get(20)?,
        )?)?),
        requested_transcript_digest: digest(required(row.get(21)?)?)?,
        selected_transcript_version_id: TranscriptVersionId::from_bytes(bytes16(required(
            row.get(22)?,
        )?)?),
        selected_transcript_digest: digest(required(row.get(23)?)?)?,
        expected_selection_revision: StateRevision::new(unsigned(required(row.get(24)?)?)?),
        base_artifact_id,
        base_integrity_digest,
        format_version: unsigned32(required(row.get(27)?)?)?,
        policy_version: unsigned32(required(row.get(28)?)?)?,
        provider: required(row.get(29)?)?,
        model: required(row.get(30)?)?,
        response_format_code: unsigned32(required(row.get(31)?)?)?,
        maximum_completion_bytes: unsigned(required(row.get(32)?)?)?,
        duration_ms: row.get::<_, Option<i64>>(33)?.map(unsigned).transpose()?,
        expected_artifact_source: artifact_source(required(row.get(34)?)?)?,
        system_prompt: required(row.get(35)?)?,
        user_prompt: required(row.get(36)?)?,
    })
}

fn required<T>(value: Option<T>) -> rusqlite::Result<T> {
    value.ok_or_else(invalid_row)
}

fn optional_id<T>(
    value: Option<Vec<u8>>,
    make: impl FnOnce([u8; 16]) -> T,
) -> rusqlite::Result<Option<T>> {
    value.map(bytes16).transpose().map(|value| value.map(make))
}

fn bytes16(value: Vec<u8>) -> rusqlite::Result<[u8; 16]> {
    value.try_into().map_err(|_| invalid_row())
}

fn digest(value: Vec<u8>) -> rusqlite::Result<ContentDigest> {
    value
        .try_into()
        .map(ContentDigest::from_bytes)
        .map_err(|_| invalid_row())
}

fn optional_digest(value: Option<Vec<u8>>) -> rusqlite::Result<Option<ContentDigest>> {
    value.map(digest).transpose()
}

fn unsigned(value: i64) -> rusqlite::Result<u64> {
    u64::try_from(value).map_err(|_| invalid_row())
}

fn unsigned16(value: i64) -> rusqlite::Result<u16> {
    u16::try_from(value).map_err(|_| invalid_row())
}

fn unsigned32(value: i64) -> rusqlite::Result<u32> {
    u32::try_from(value).map_err(|_| invalid_row())
}

fn bool_value(value: i64) -> rusqlite::Result<bool> {
    match value {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(invalid_row()),
    }
}

fn artifact_source(value: i64) -> rusqlite::Result<ChapterArtifactSource> {
    match value {
        1 => Ok(ChapterArtifactSource::Publisher),
        2 => Ok(ChapterArtifactSource::Generated),
        3 => Ok(ChapterArtifactSource::PublisherEnriched),
        4 => Ok(ChapterArtifactSource::AgentComposed),
        _ => Err(invalid_row()),
    }
}

fn invalid_row() -> rusqlite::Error {
    rusqlite::Error::InvalidQuery
}
