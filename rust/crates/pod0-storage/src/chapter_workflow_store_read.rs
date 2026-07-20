use pod0_domain::{
    CancellationId, ChapterArtifactId, CommandId, EpisodeId, HostRequestId, StateRevision,
};
use rusqlite::{Connection, OptionalExtension, Row, params};

use crate::{
    PublisherChapterWorkflowPage, PublisherChapterWorkflowRecord, PublisherChapterWorkflowState,
    StorageError,
};

pub(crate) const WORKFLOW_COLUMNS: &str = "episode_id,source_url,source_version,state,generation,workflow_revision,attempt,max_attempts,\
     command_id,cancellation_id,request_id,issued_revision,expected_selection_revision,\
     deadline_at_ms,not_before_ms,selected_artifact_id,failure_code,failure_detail,\
     created_at_ms,updated_at_ms";

pub(crate) fn read_workflow(
    connection: &Connection,
    episode_id: EpisodeId,
) -> Result<Option<PublisherChapterWorkflowRecord>, StorageError> {
    connection
        .query_row(
            &format!(
                "SELECT {WORKFLOW_COLUMNS} FROM pod0_publisher_chapter_workflows WHERE episode_id=?1"
            ),
            [episode_id.into_bytes().as_slice()],
            row,
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read publisher chapter workflow", error))
}

pub(crate) fn read_active_workflows(
    connection: &Connection,
    max_items: u16,
) -> Result<Vec<PublisherChapterWorkflowRecord>, StorageError> {
    let mut statement = connection
        .prepare(&format!(
            "SELECT {WORKFLOW_COLUMNS} FROM pod0_publisher_chapter_workflows \
             WHERE state IN('requested','retry_scheduled') \
             ORDER BY not_before_ms,episode_id LIMIT ?1"
        ))
        .map_err(|error| StorageError::sqlite("prepare active publisher workflows", error))?;
    let rows = statement
        .query_map([i64::from(max_items.max(1))], row)
        .map_err(|error| StorageError::sqlite("query active publisher workflows", error))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| StorageError::sqlite("decode active publisher workflows", error))
}

pub(crate) fn read_workflow_page(
    connection: &Connection,
    episode_id: Option<EpisodeId>,
    offset: u32,
    max_items: u16,
) -> Result<PublisherChapterWorkflowPage, StorageError> {
    let limit = usize::from(max_items.max(1));
    let fetch = i64::try_from(limit + 1).expect("bounded publisher workflow page limit");
    let mut statement = connection
        .prepare(&format!(
            "SELECT {WORKFLOW_COLUMNS} FROM pod0_publisher_chapter_workflows \
             WHERE state!='source_absent' AND (?1 IS NULL OR episode_id=?1) \
             ORDER BY updated_at_ms DESC,episode_id LIMIT ?2 OFFSET ?3"
        ))
        .map_err(|error| StorageError::sqlite("prepare bounded publisher workflows", error))?;
    let rows = statement
        .query_map(
            params![
                episode_id.map(|id| id.into_bytes().to_vec()),
                fetch,
                i64::from(offset)
            ],
            row,
        )
        .map_err(|error| StorageError::sqlite("query bounded publisher workflows", error))?;
    let mut items = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| StorageError::sqlite("decode bounded publisher workflows", error))?;
    let has_more = items.len() > limit;
    items.truncate(limit);
    Ok(PublisherChapterWorkflowPage { items, has_more })
}

fn row(row: &Row<'_>) -> rusqlite::Result<PublisherChapterWorkflowRecord> {
    let episode: Vec<u8> = row.get(0)?;
    let state: String = row.get(3)?;
    let command: Vec<u8> = row.get(8)?;
    let cancellation: Vec<u8> = row.get(9)?;
    let request: Option<Vec<u8>> = row.get(10)?;
    let artifact: Option<Vec<u8>> = row.get(15)?;
    Ok(PublisherChapterWorkflowRecord {
        episode_id: EpisodeId::from_bytes(bytes16(episode)?),
        source_url: row.get(1)?,
        source_version: row.get(2)?,
        state: PublisherChapterWorkflowState::parse(&state).ok_or_else(invalid_row)?,
        generation: unsigned(row.get(4)?)?,
        workflow_revision: StateRevision::new(unsigned(row.get(5)?)?),
        attempt: unsigned16(row.get(6)?)?,
        max_attempts: unsigned16(row.get(7)?)?,
        command_id: CommandId::from_bytes(bytes16(command)?),
        cancellation_id: CancellationId::from_bytes(bytes16(cancellation)?),
        request_id: request
            .map(bytes16)
            .transpose()?
            .map(HostRequestId::from_bytes),
        issued_revision: StateRevision::new(unsigned(row.get(11)?)?),
        expected_selection_revision: StateRevision::new(unsigned(row.get(12)?)?),
        deadline_at_ms: row.get(13)?,
        not_before_ms: row.get(14)?,
        selected_artifact_id: artifact
            .map(bytes16)
            .transpose()?
            .map(ChapterArtifactId::from_bytes),
        failure_code: row.get(16)?,
        failure_detail: row.get(17)?,
        created_at_ms: row.get(18)?,
        updated_at_ms: row.get(19)?,
    })
}

fn bytes16(bytes: Vec<u8>) -> rusqlite::Result<[u8; 16]> {
    bytes.try_into().map_err(|_| invalid_row())
}

fn unsigned(value: i64) -> rusqlite::Result<u64> {
    u64::try_from(value).map_err(|_| invalid_row())
}

fn unsigned16(value: i64) -> rusqlite::Result<u16> {
    u16::try_from(value).map_err(|_| invalid_row())
}

fn invalid_row() -> rusqlite::Error {
    rusqlite::Error::InvalidQuery
}
