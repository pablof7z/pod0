use pod0_domain::{
    CancellationId, CommandId, DownloadAttemptId, DownloadIntentId, EpisodeId, HostRequestId,
    StateRevision,
};
use rusqlite::{Connection, OptionalExtension, Row, params};

use crate::{
    DownloadEnvironmentRecord, DownloadHostRequestKind, DownloadHostRequestRecord,
    DownloadWorkflowPage, DownloadWorkflowRecord, StorageError, StoredDownloadDesiredState,
    StoredDownloadNetwork, StoredDownloadOrigin, StoredDownloadStage,
};

const WORKFLOW_COLUMNS: &str = "episode_id,intent_id,input_version,origin_code,origin_wire_code,\
    desired_state,stage,workflow_revision,attempt,attempt_id,request_id,command_id,cancellation_id,\
    issued_revision,deadline_at_ms,not_before_ms,enclosure_url,resume_key,external_task_key,\
    artifact_key,artifact_byte_count,artifact_digest,failure_code,failure_detail,failure_retryable,\
    created_at_ms,updated_at_ms";

const REQUEST_COLUMNS: &str = "request_id,episode_id,kind,command_id,cancellation_id,issued_revision,\
    deadline_at_ms,intent_id,attempt_id,input_version,enclosure_url,resume_key,external_task_key,\
    artifact_key,last_sequence_number";

pub(crate) fn workflow(
    connection: &Connection,
    episode_id: EpisodeId,
) -> Result<Option<DownloadWorkflowRecord>, StorageError> {
    connection
        .query_row(
            &format!("SELECT {WORKFLOW_COLUMNS} FROM pod0_download_workflows WHERE episode_id=?1"),
            [episode_id.into_bytes().as_slice()],
            workflow_row,
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read download workflow", error))
}

pub(crate) fn workflow_for_request(
    connection: &Connection,
    request_id: HostRequestId,
) -> Result<DownloadWorkflowRecord, StorageError> {
    connection
        .query_row(
            &format!("SELECT {WORKFLOW_COLUMNS} FROM pod0_download_workflows WHERE request_id=?1"),
            [request_id.into_bytes().as_slice()],
            workflow_row,
        )
        .optional()
        .map_err(|error| StorageError::sqlite("find download workflow request", error))?
        .ok_or(StorageError::DownloadWorkflowNotFound)
}

pub(crate) fn page(
    connection: &Connection,
    episode_id: Option<EpisodeId>,
    offset: u32,
    max_items: u16,
) -> Result<DownloadWorkflowPage, StorageError> {
    let limit = usize::from(max_items.max(1));
    let fetch = i64::try_from(limit + 1).expect("bounded download page");
    let mut statement = connection
        .prepare(&format!(
            "SELECT {WORKFLOW_COLUMNS} FROM pod0_download_workflows \
             WHERE (?1 IS NULL OR episode_id=?1) \
             ORDER BY updated_at_ms DESC,episode_id LIMIT ?2 OFFSET ?3"
        ))
        .map_err(|error| StorageError::sqlite("prepare download workflow page", error))?;
    let rows = statement
        .query_map(
            params![
                episode_id.map(|id| id.into_bytes().to_vec()),
                fetch,
                i64::from(offset)
            ],
            workflow_row,
        )
        .map_err(|error| StorageError::sqlite("query download workflow page", error))?;
    let mut items = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| StorageError::sqlite("decode download workflow page", error))?;
    let has_more = items.len() > limit;
    items.truncate(limit);
    Ok(DownloadWorkflowPage { items, has_more })
}

pub(crate) fn pending_requests(
    connection: &Connection,
    max_items: u16,
) -> Result<Vec<DownloadHostRequestRecord>, StorageError> {
    let mut statement = connection
        .prepare(&format!(
            "SELECT {REQUEST_COLUMNS} FROM pod0_download_host_requests \
             WHERE state='pending' ORDER BY created_at_ms,request_id LIMIT ?1"
        ))
        .map_err(|error| StorageError::sqlite("prepare pending download requests", error))?;
    let rows = statement
        .query_map([i64::from(max_items.max(1))], request_row)
        .map_err(|error| StorageError::sqlite("query pending download requests", error))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| StorageError::sqlite("decode pending download requests", error))
}

pub(crate) fn request(
    connection: &Connection,
    request_id: HostRequestId,
) -> Result<Option<(DownloadHostRequestRecord, String)>, StorageError> {
    connection
        .query_row(
            &format!(
                "SELECT {REQUEST_COLUMNS},state FROM pod0_download_host_requests WHERE request_id=?1"
            ),
            [request_id.into_bytes().as_slice()],
            |row| Ok((request_row(row)?, row.get(15)?)),
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read download host request", error))
}

pub(crate) fn environment(
    connection: &Connection,
) -> Result<DownloadEnvironmentRecord, StorageError> {
    connection
        .query_row(
            "SELECT network_code,network_wire_code,available_capacity_bytes,observed_at_ms \
             FROM pod0_download_environment WHERE singleton=1",
            [],
            |row| {
                let network = StoredDownloadNetwork::parse(row.get(0)?, row.get(1)?)
                    .ok_or_else(invalid_row)?;
                Ok(DownloadEnvironmentRecord {
                    network,
                    available_capacity_bytes: optional_unsigned(row.get(2)?)?,
                    observed_at_ms: row.get(3)?,
                })
            },
        )
        .map_err(|error| StorageError::sqlite("read download environment", error))
}

fn workflow_row(row: &Row<'_>) -> rusqlite::Result<DownloadWorkflowRecord> {
    let origin = StoredDownloadOrigin::parse(row.get(3)?, row.get(4)?).ok_or_else(invalid_row)?;
    let desired =
        StoredDownloadDesiredState::parse(&row.get::<_, String>(5)?).ok_or_else(invalid_row)?;
    let stage = StoredDownloadStage::parse(&row.get::<_, String>(6)?).ok_or_else(invalid_row)?;
    Ok(DownloadWorkflowRecord {
        episode_id: EpisodeId::from_bytes(bytes::<16>(row.get(0)?)?),
        intent_id: DownloadIntentId::from_bytes(bytes::<16>(row.get(1)?)?),
        input_version: row.get(2)?,
        origin,
        desired_state: desired,
        stage,
        workflow_revision: StateRevision::new(unsigned(row.get(7)?)?),
        attempt: u16::try_from(row.get::<_, i64>(8)?).map_err(|_| invalid_row())?,
        attempt_id: optional_id(row.get(9)?, DownloadAttemptId::from_bytes)?,
        request_id: optional_id(row.get(10)?, HostRequestId::from_bytes)?,
        command_id: CommandId::from_bytes(bytes::<16>(row.get(11)?)?),
        cancellation_id: CancellationId::from_bytes(bytes::<16>(row.get(12)?)?),
        issued_revision: StateRevision::new(unsigned(row.get(13)?)?),
        deadline_at_ms: row.get(14)?,
        not_before_ms: row.get(15)?,
        enclosure_url: row.get(16)?,
        resume_key: row.get(17)?,
        external_task_key: row.get(18)?,
        artifact_key: row.get(19)?,
        artifact_byte_count: optional_unsigned(row.get(20)?)?,
        artifact_digest: row
            .get::<_, Option<Vec<u8>>>(21)?
            .map(bytes::<32>)
            .transpose()?,
        failure_code: row.get(22)?,
        failure_detail: row.get(23)?,
        failure_retryable: row.get::<_, i64>(24)? == 1,
        created_at_ms: row.get(25)?,
        updated_at_ms: row.get(26)?,
    })
}

fn request_row(row: &Row<'_>) -> rusqlite::Result<DownloadHostRequestRecord> {
    let kind = match row.get::<_, String>(2)?.as_str() {
        "start" => DownloadHostRequestKind::Start,
        "cancel" => DownloadHostRequestKind::Cancel,
        "remove" => DownloadHostRequestKind::Remove,
        _ => return Err(invalid_row()),
    };
    Ok(DownloadHostRequestRecord {
        request_id: HostRequestId::from_bytes(bytes::<16>(row.get(0)?)?),
        episode_id: EpisodeId::from_bytes(bytes::<16>(row.get(1)?)?),
        kind,
        command_id: CommandId::from_bytes(bytes::<16>(row.get(3)?)?),
        cancellation_id: CancellationId::from_bytes(bytes::<16>(row.get(4)?)?),
        issued_revision: StateRevision::new(unsigned(row.get(5)?)?),
        deadline_at_ms: row.get(6)?,
        intent_id: optional_id(row.get(7)?, DownloadIntentId::from_bytes)?,
        attempt_id: optional_id(row.get(8)?, DownloadAttemptId::from_bytes)?,
        input_version: row.get(9)?,
        enclosure_url: row.get(10)?,
        resume_key: row.get(11)?,
        external_task_key: row.get(12)?,
        artifact_key: row.get(13)?,
        last_sequence_number: optional_unsigned(row.get(14)?)?,
    })
}

fn optional_id<T>(
    value: Option<Vec<u8>>,
    make: impl FnOnce([u8; 16]) -> T,
) -> rusqlite::Result<Option<T>> {
    value.map(|value| bytes::<16>(value).map(make)).transpose()
}

fn bytes<const N: usize>(value: Vec<u8>) -> rusqlite::Result<[u8; N]> {
    value.try_into().map_err(|_| invalid_row())
}

fn unsigned(value: i64) -> rusqlite::Result<u64> {
    u64::try_from(value).map_err(|_| invalid_row())
}

fn optional_unsigned(value: Option<i64>) -> rusqlite::Result<Option<u64>> {
    value.map(unsigned).transpose()
}

fn invalid_row() -> rusqlite::Error {
    rusqlite::Error::InvalidQuery
}
