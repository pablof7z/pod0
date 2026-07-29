use pod0_domain::{
    CancellationId, CommandId, FeedIdentityV1, HostRequestId, PodcastId, PodcastKind,
    PodcastRecord, StateRevision, UnixTimestampMilliseconds,
};
use rusqlite::{Connection, OptionalExtension, Transaction, params};

use crate::StorageError;
use crate::download_store_request::derived_request_id;
use crate::feed_fetch_store_model::{
    FeedFetchEnsureInput, FeedFetchEnsureOutcome, FeedFetchFailureInput, FeedFetchWorkflowRecord,
    StoredFeedFetchIntent, StoredFeedFetchStage,
};
use crate::library_store::LibraryStore;
use crate::library_store_feed::{insert_subscription, upsert_podcast};

impl LibraryStore {
    /// Commits the feed intent durably in one transaction: the placeholder
    /// podcast, the subscription when subscribing, and the workflow row the
    /// host request is issued from. The `feed_key_v1` primary key coalesces
    /// concurrent intents for one normalized feed identity onto one workflow.
    pub fn ensure_feed_fetch_workflow(
        &self,
        input: FeedFetchEnsureInput,
    ) -> Result<FeedFetchEnsureOutcome, StorageError> {
        self.write(|transaction| {
            let existing = podcast_id_for_feed_key(transaction, &input.feed_key)?;
            let podcast_id = existing.unwrap_or(input.podcast_id);
            if existing.is_none() {
                upsert_podcast(transaction, &placeholder_podcast(&input, podcast_id))?;
            }
            if input.intent == StoredFeedFetchIntent::Subscribe {
                insert_subscription(transaction, podcast_id, input.now_ms)?;
            }
            if let Some(active) = workflow_for_feed(transaction, &input.feed_key)?
                && active.stage != StoredFeedFetchStage::Failed
            {
                if input.intent > active.intent {
                    transaction
                        .execute(
                            "UPDATE pod0_feed_fetch_workflows SET intent=?1,updated_at_ms=?2 \
                             WHERE feed_key_v1=?3",
                            params![input.intent.wire(), input.now_ms, input.feed_key],
                        )
                        .map_err(|error| {
                            StorageError::sqlite("coalesce feed fetch intent", error)
                        })?;
                }
                let record = workflow_for_feed(transaction, &input.feed_key)?;
                return Ok(FeedFetchEnsureOutcome { podcast_id, record });
            }
            let request_id = feed_fetch_request_id(&input.feed_key, input.command_id, 1);
            transaction
                .execute(
                    "INSERT INTO pod0_feed_fetch_workflows(feed_key_v1,source_url,podcast_id,\
                     intent,stage,attempt,request_id,command_id,command_fingerprint,\
                     cancellation_id,issued_revision,deadline_at_ms,not_before_ms,entity_tag,\
                     last_modified,failure_code,created_at_ms,updated_at_ms) \
                     VALUES(?1,?2,?3,?4,'requested',1,?5,?6,?7,?8,?9,?10,NULL,?11,?12,NULL,\
                     ?13,?13) ON CONFLICT(feed_key_v1) DO UPDATE SET \
                     source_url=excluded.source_url,podcast_id=excluded.podcast_id,\
                     intent=excluded.intent,stage='requested',attempt=1,\
                     request_id=excluded.request_id,command_id=excluded.command_id,\
                     command_fingerprint=excluded.command_fingerprint,\
                     cancellation_id=excluded.cancellation_id,\
                     issued_revision=excluded.issued_revision,\
                     deadline_at_ms=excluded.deadline_at_ms,not_before_ms=NULL,\
                     entity_tag=excluded.entity_tag,last_modified=excluded.last_modified,\
                     failure_code=NULL,updated_at_ms=excluded.updated_at_ms",
                    params![
                        input.feed_key,
                        input.source_url,
                        podcast_id.into_bytes().as_slice(),
                        input.intent.wire(),
                        request_id.into_bytes().as_slice(),
                        input.command_id.into_bytes().as_slice(),
                        input.command_fingerprint,
                        input.cancellation_id.into_bytes().as_slice(),
                        revision_value(input.issued_revision)?,
                        input.deadline_at_ms,
                        input.entity_tag,
                        input.last_modified,
                        input.now_ms
                    ],
                )
                .map_err(|error| StorageError::sqlite("insert feed fetch workflow", error))?;
            let record = workflow_for_feed(transaction, &input.feed_key)?;
            Ok(FeedFetchEnsureOutcome { podcast_id, record })
        })
    }

    /// Transitions the workflow after a failed fetch attempt. When
    /// `retry_at_ms` is provided the next attempt is scheduled durably;
    /// otherwise the row parks in stage `failed` until a new command
    /// replaces it.
    pub fn fail_feed_fetch_workflow(
        &self,
        input: FeedFetchFailureInput,
    ) -> Result<Option<FeedFetchWorkflowRecord>, StorageError> {
        self.write(|transaction| {
            let Some(row) = workflow_for_request(transaction, input.request_id)? else {
                return Ok(None);
            };
            if row.stage == StoredFeedFetchStage::Failed {
                return Ok(None);
            }
            if input.retryable
                && let Some(retry_at) = input.retry_at_ms
            {
                let attempt = row
                    .attempt
                    .checked_add(1)
                    .ok_or(StorageError::CommandConflict)?;
                let request_id = feed_fetch_request_id(&row.feed_key, row.command_id, attempt);
                transaction
                    .execute(
                        "UPDATE pod0_feed_fetch_workflows SET stage='retry_scheduled',\
                         attempt=?1,request_id=?2,issued_revision=?3,deadline_at_ms=?4,\
                         not_before_ms=?5,failure_code=?6,updated_at_ms=?7 WHERE request_id=?8",
                        params![
                            i64::from(attempt),
                            request_id.into_bytes().as_slice(),
                            revision_value(input.issued_revision)?,
                            input.retry_deadline_at_ms,
                            retry_at,
                            input.failure_code,
                            input.observed_at_ms,
                            input.request_id.into_bytes().as_slice()
                        ],
                    )
                    .map_err(|error| StorageError::sqlite("schedule feed fetch retry", error))?;
                return workflow_for_request(transaction, request_id);
            }
            transaction
                .execute(
                    "UPDATE pod0_feed_fetch_workflows SET stage='failed',not_before_ms=NULL,\
                     failure_code=?1,updated_at_ms=?2 WHERE request_id=?3",
                    params![
                        input.failure_code,
                        input.observed_at_ms,
                        input.request_id.into_bytes().as_slice()
                    ],
                )
                .map_err(|error| StorageError::sqlite("fail feed fetch workflow", error))?;
            workflow_for_request(transaction, input.request_id)
        })
    }

    /// Removes the workflow row once its fetch has been applied (or the
    /// intent was cancelled). Idempotent.
    pub fn complete_feed_fetch_workflow(
        &self,
        request_id: HostRequestId,
    ) -> Result<bool, StorageError> {
        self.write(|transaction| {
            transaction
                .execute(
                    "DELETE FROM pod0_feed_fetch_workflows WHERE request_id=?1",
                    [request_id.into_bytes().as_slice()],
                )
                .map_err(|error| StorageError::sqlite("complete feed fetch workflow", error))?;
            Ok(transaction.changes() == 1)
        })
    }

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

pub(crate) fn feed_fetch_request_id(
    feed_key: &str,
    command_id: CommandId,
    attempt: u16,
) -> HostRequestId {
    let mut identity = Vec::with_capacity(feed_key.len() + 16);
    identity.extend_from_slice(feed_key.as_bytes());
    identity.extend_from_slice(&command_id.into_bytes());
    derived_request_id(b"pod0-feed-fetch-request-v1", &identity, u64::from(attempt))
}

fn placeholder_podcast(input: &FeedFetchEnsureInput, podcast_id: PodcastId) -> PodcastRecord {
    PodcastRecord {
        podcast_id,
        kind: PodcastKind::Rss,
        feed_identity: Some(FeedIdentityV1 {
            source_url: input.source_url.clone(),
            comparison_key: input.feed_key.clone(),
        }),
        title: input.placeholder_title.clone(),
        author: String::new(),
        image_url: None,
        description: String::new(),
        language: None,
        categories: Vec::new(),
        discovered_at: UnixTimestampMilliseconds::new(input.now_ms),
        title_is_placeholder: true,
        last_refreshed_at: None,
        etag: None,
        last_modified: None,
    }
}

fn revision_value(revision: StateRevision) -> Result<i64, StorageError> {
    i64::try_from(revision.value).map_err(|_| StorageError::CommandConflict)
}

fn podcast_id_for_feed_key(
    transaction: &Transaction<'_>,
    feed_key: &str,
) -> Result<Option<PodcastId>, StorageError> {
    let stored: Option<Vec<u8>> = transaction
        .query_row(
            "SELECT podcast_id FROM pod0_podcasts WHERE feed_key_v1=?1",
            [feed_key],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| StorageError::sqlite("resolve feed fetch podcast", error))?;
    stored
        .map(|bytes| {
            let bytes: [u8; 16] = bytes.try_into().map_err(|_| StorageError::CorruptSchema {
                detail: "feed fetch podcast identity is malformed",
            })?;
            Ok(PodcastId::from_bytes(bytes))
        })
        .transpose()
}

fn workflow_for_feed(
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

fn workflow_for_request(
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
