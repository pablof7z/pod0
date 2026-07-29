use pod0_domain::{CancellationId, CommandId, HostRequestId, PodcastId, StateRevision};
use rusqlite::{Connection, OptionalExtension, Transaction};

use crate::StorageError;
use crate::feed_fetch_store_model::{
    FeedFetchWorkflowRecord, StoredFeedFetchIntent, StoredFeedFetchStage,
};
use crate::library_store::LibraryStore;

impl LibraryStore {
    /// Active rows (requested or retry-scheduled) that may owe a host request.
    pub fn active_feed_fetch_workflows(
        &self,
        limit: u16,
    ) -> Result<Vec<FeedFetchWorkflowRecord>, StorageError> {
        self.read(|connection| {
            read_workflows(
                connection,
                "SELECT feed_key_v1,source_url,podcast_id,intent,stage,attempt,request_id,\
                 command_id,command_fingerprint,cancellation_id,issued_revision,deadline_at_ms,\
                 not_before_ms,entity_tag,last_modified,failure_code,updated_at_ms \
                 FROM pod0_feed_fetch_workflows WHERE stage IN('requested','retry_scheduled') \
                 ORDER BY updated_at_ms,feed_key_v1 LIMIT ?1",
                limit,
            )
        })
    }

    /// Every stored workflow row, including terminal failures, for projection.
    pub fn feed_fetch_workflows_snapshot(
        &self,
        limit: u16,
    ) -> Result<Vec<FeedFetchWorkflowRecord>, StorageError> {
        self.read(|connection| {
            read_workflows(
                connection,
                "SELECT feed_key_v1,source_url,podcast_id,intent,stage,attempt,request_id,\
                 command_id,command_fingerprint,cancellation_id,issued_revision,deadline_at_ms,\
                 not_before_ms,entity_tag,last_modified,failure_code,updated_at_ms \
                 FROM pod0_feed_fetch_workflows ORDER BY updated_at_ms,feed_key_v1 LIMIT ?1",
                limit,
            )
        })
    }
}

pub(crate) fn workflow_for_feed(
    transaction: &Transaction<'_>,
    feed_key: &str,
) -> Result<Option<FeedFetchWorkflowRecord>, StorageError> {
    transaction
        .query_row(
            "SELECT feed_key_v1,source_url,podcast_id,intent,stage,attempt,request_id,\
             command_id,command_fingerprint,cancellation_id,issued_revision,deadline_at_ms,\
             not_before_ms,entity_tag,last_modified,failure_code,updated_at_ms \
             FROM pod0_feed_fetch_workflows WHERE feed_key_v1=?1",
            [feed_key],
            decode_row,
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read feed fetch workflow", error))?
        .map(finish_decode)
        .transpose()
}

pub(crate) fn workflow_for_request(
    transaction: &Transaction<'_>,
    request_id: HostRequestId,
) -> Result<Option<FeedFetchWorkflowRecord>, StorageError> {
    transaction
        .query_row(
            "SELECT feed_key_v1,source_url,podcast_id,intent,stage,attempt,request_id,\
             command_id,command_fingerprint,cancellation_id,issued_revision,deadline_at_ms,\
             not_before_ms,entity_tag,last_modified,failure_code,updated_at_ms \
             FROM pod0_feed_fetch_workflows WHERE request_id=?1",
            [request_id.into_bytes().as_slice()],
            decode_row,
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read feed fetch workflow request", error))?
        .map(finish_decode)
        .transpose()
}

fn read_workflows(
    connection: &Connection,
    sql: &str,
    limit: u16,
) -> Result<Vec<FeedFetchWorkflowRecord>, StorageError> {
    let mut statement = connection
        .prepare(sql)
        .map_err(|error| StorageError::sqlite("prepare feed fetch workflows", error))?;
    let rows = statement
        .query_map([i64::from(limit)], decode_row)
        .map_err(|error| StorageError::sqlite("read feed fetch workflows", error))?;
    let mut records = Vec::new();
    for row in rows {
        let raw = row.map_err(|error| StorageError::sqlite("decode feed fetch workflow", error))?;
        records.push(finish_decode(raw)?);
    }
    Ok(records)
}

type RawWorkflowRow = (
    String,
    String,
    Vec<u8>,
    String,
    String,
    i64,
    Vec<u8>,
    Vec<u8>,
    String,
    Vec<u8>,
    i64,
    Option<i64>,
    Option<i64>,
    Option<String>,
    Option<String>,
    Option<String>,
    i64,
);

#[allow(clippy::type_complexity)]
fn decode_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<RawWorkflowRow> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
        row.get(6)?,
        row.get(7)?,
        row.get(8)?,
        row.get(9)?,
        row.get(10)?,
        row.get(11)?,
        row.get(12)?,
        row.get(13)?,
        row.get(14)?,
        row.get(15)?,
        row.get(16)?,
    ))
}

fn finish_decode(raw: RawWorkflowRow) -> Result<FeedFetchWorkflowRecord, StorageError> {
    let malformed = || StorageError::CorruptSchema {
        detail: "feed fetch workflow row is malformed",
    };
    let (
        feed_key,
        source_url,
        podcast_id,
        intent,
        stage,
        attempt,
        request_id,
        command_id,
        command_fingerprint,
        cancellation_id,
        issued_revision,
        deadline_at_ms,
        not_before_ms,
        entity_tag,
        last_modified,
        failure_code,
        updated_at_ms,
    ) = raw;
    Ok(FeedFetchWorkflowRecord {
        feed_key,
        source_url,
        podcast_id: PodcastId::from_bytes(podcast_id.try_into().map_err(|_| malformed())?),
        intent: StoredFeedFetchIntent::parse(&intent).ok_or_else(malformed)?,
        stage: StoredFeedFetchStage::parse(&stage).ok_or_else(malformed)?,
        attempt: u16::try_from(attempt).map_err(|_| malformed())?,
        request_id: HostRequestId::from_bytes(request_id.try_into().map_err(|_| malformed())?),
        command_id: CommandId::from_bytes(command_id.try_into().map_err(|_| malformed())?),
        command_fingerprint,
        cancellation_id: CancellationId::from_bytes(
            cancellation_id.try_into().map_err(|_| malformed())?,
        ),
        issued_revision: StateRevision::new(
            u64::try_from(issued_revision).map_err(|_| malformed())?,
        ),
        deadline_at_ms,
        not_before_ms,
        entity_tag,
        last_modified,
        failure_code,
        updated_at_ms,
    })
}
