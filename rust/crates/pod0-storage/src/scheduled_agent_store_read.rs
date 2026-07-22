use pod0_application::{
    MAX_SCHEDULED_AGENT_TASKS, ScheduledAgentExecutionRequest, ScheduledAgentOccurrenceState,
    ScheduledTaskDefinition,
};
use pod0_domain::{ScheduledOccurrenceId, ScheduledTaskId, UnixTimestampMilliseconds};
use rusqlite::{Connection, OptionalExtension, Row, params};

use crate::scheduled_agent_store_codec as codec;
use crate::{
    ScheduledAgentHostRequestRecord, ScheduledOccurrencePage, ScheduledTaskPage, StorageError,
};

pub(crate) fn read_task(
    connection: &Connection,
    task_id: ScheduledTaskId,
    active_only: bool,
) -> Result<Option<ScheduledTaskDefinition>, StorageError> {
    connection
        .query_row(
            "SELECT task_id,label,prompt,prompt_revision,model_reference,interval_ms,created_at_ms,\
             last_run_at_ms,next_run_at_ms,task_revision FROM pod0_scheduled_tasks \
             WHERE task_id=?1 AND (?2=0 OR active=1)",
            params![task_id.into_bytes().as_slice(), active_only],
            decode_task,
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read scheduled task", error))?
        .transpose()
}

pub(crate) fn active_tasks(
    connection: &Connection,
) -> Result<Vec<ScheduledTaskDefinition>, StorageError> {
    let mut statement = connection
        .prepare(
            "SELECT task_id,label,prompt,prompt_revision,model_reference,interval_ms,created_at_ms,\
             last_run_at_ms,next_run_at_ms,task_revision FROM pod0_scheduled_tasks \
             WHERE active=1 ORDER BY next_run_at_ms,task_id",
        )
        .map_err(|error| StorageError::sqlite("prepare active scheduled tasks", error))?;
    collect(
        statement
            .query_map([], decode_task)
            .map_err(|error| StorageError::sqlite("query active scheduled tasks", error))?,
        "read active scheduled tasks",
    )
}

pub(crate) fn read_occurrence(
    connection: &Connection,
    occurrence_id: ScheduledOccurrenceId,
) -> Result<Option<ScheduledAgentOccurrenceState>, StorageError> {
    connection
        .query_row(
            "SELECT task_id,occurrence_id,prompt,prompt_revision,model_reference,stage,\
             workflow_revision,attempt,attempt_id,request_id,provider_operation_id,not_before_ms,\
             artifact_id,output_digest,failure_code,failure_wire_code,failure_detail,\
             failure_retryable,updated_at_ms FROM pod0_scheduled_occurrences WHERE occurrence_id=?1",
            [occurrence_id.into_bytes().as_slice()],
            decode_occurrence,
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read scheduled occurrence", error))?
        .transpose()
}

pub(crate) fn pending_requests(
    connection: &Connection,
    command_id: Option<pod0_domain::CommandId>,
    max_items: u16,
) -> Result<Vec<ScheduledAgentHostRequestRecord>, StorageError> {
    let limit = i64::from(max_items.clamp(1, MAX_SCHEDULED_AGENT_TASKS));
    let mut statement = connection
        .prepare(
            "SELECT a.request_id,a.command_id,a.cancellation_id,a.issued_revision,a.deadline_at_ms,\
             o.occurrence_id,a.attempt_id,o.prompt_revision,o.prompt,o.model_reference,\
             a.last_sequence_number FROM pod0_scheduled_attempts a \
             JOIN pod0_scheduled_occurrences o ON o.occurrence_id=a.occurrence_id \
             WHERE a.state='requested' AND (?1 IS NULL OR a.command_id=?1) \
             ORDER BY a.created_at_ms,a.request_id LIMIT ?2",
        )
        .map_err(|error| StorageError::sqlite("prepare scheduled host requests", error))?;
    let command_bytes = command_id.map(|value| value.into_bytes().to_vec());
    collect(
        statement
            .query_map(params![command_bytes, limit], decode_request)
            .map_err(|error| StorageError::sqlite("query scheduled host requests", error))?,
        "read scheduled host requests",
    )
}

pub(crate) fn task_page(
    connection: &Connection,
    offset: u32,
    max_items: u16,
) -> Result<ScheduledTaskPage, StorageError> {
    let limit = i64::from(max_items.clamp(1, MAX_SCHEDULED_AGENT_TASKS));
    let mut statement = connection
        .prepare(
            "SELECT task_id,label,prompt,prompt_revision,model_reference,interval_ms,created_at_ms,\
             last_run_at_ms,next_run_at_ms,task_revision FROM pod0_scheduled_tasks WHERE active=1 \
             ORDER BY next_run_at_ms,task_id LIMIT ?1 OFFSET ?2",
        )
        .map_err(|error| StorageError::sqlite("prepare scheduled task page", error))?;
    let items = collect(
        statement
            .query_map(params![limit + 1, offset], decode_task)
            .map_err(|error| StorageError::sqlite("query scheduled task page", error))?,
        "read scheduled task page",
    )?;
    Ok(page_tasks(items, limit as usize))
}

pub(crate) fn occurrence_page(
    connection: &Connection,
    task_id: Option<ScheduledTaskId>,
    offset: u32,
    max_items: u16,
) -> Result<ScheduledOccurrencePage, StorageError> {
    let limit = i64::from(max_items.clamp(1, MAX_SCHEDULED_AGENT_TASKS));
    let mut statement = connection
        .prepare(
            "SELECT task_id,occurrence_id,prompt,prompt_revision,model_reference,stage,\
             workflow_revision,attempt,attempt_id,request_id,provider_operation_id,not_before_ms,\
             artifact_id,output_digest,failure_code,failure_wire_code,failure_detail,\
             failure_retryable,updated_at_ms FROM pod0_scheduled_occurrences \
             WHERE (?1 IS NULL OR task_id=?1) ORDER BY updated_at_ms DESC,occurrence_id \
             LIMIT ?2 OFFSET ?3",
        )
        .map_err(|error| StorageError::sqlite("prepare scheduled occurrence page", error))?;
    let task_bytes = task_id.map(|value| value.into_bytes().to_vec());
    let items = collect(
        statement
            .query_map(params![task_bytes, limit + 1, offset], decode_occurrence)
            .map_err(|error| StorageError::sqlite("query scheduled occurrence page", error))?,
        "read scheduled occurrence page",
    )?;
    let has_more = items.len() > limit as usize;
    Ok(ScheduledOccurrencePage {
        items: items.into_iter().take(limit as usize).collect(),
        has_more,
    })
}

fn decode_task(row: &Row<'_>) -> rusqlite::Result<Result<ScheduledTaskDefinition, StorageError>> {
    let task: Vec<u8> = row.get(0)?;
    let prompt_revision: Vec<u8> = row.get(3)?;
    let interval: i64 = row.get(5)?;
    let revision: i64 = row.get(9)?;
    Ok((|| {
        Ok(ScheduledTaskDefinition {
            task_id: codec::task_id(&task)?,
            label: row.get(1)?,
            prompt: row.get(2)?,
            prompt_revision: codec::digest(&prompt_revision)?,
            model_reference: row.get(4)?,
            interval_milliseconds: u64::try_from(interval)
                .map_err(|_| corrupt("scheduled interval"))?,
            created_at: UnixTimestampMilliseconds::new(row.get(6)?),
            last_run_at: row
                .get::<_, Option<i64>>(7)?
                .map(UnixTimestampMilliseconds::new),
            next_run_at: UnixTimestampMilliseconds::new(row.get(8)?),
            revision: codec::revision(revision)?,
        })
    })())
}

fn decode_occurrence(
    row: &Row<'_>,
) -> rusqlite::Result<Result<ScheduledAgentOccurrenceState, StorageError>> {
    let task: Vec<u8> = row.get(0)?;
    let occurrence: Vec<u8> = row.get(1)?;
    let prompt_revision: Vec<u8> = row.get(3)?;
    let stage: String = row.get(5)?;
    let revision: i64 = row.get(6)?;
    let attempt: i64 = row.get(7)?;
    let attempt_id: Option<Vec<u8>> = row.get(8)?;
    let request_id: Option<Vec<u8>> = row.get(9)?;
    let artifact_id: Option<Vec<u8>> = row.get(12)?;
    let output_digest: Option<Vec<u8>> = row.get(13)?;
    Ok((|| {
        Ok(ScheduledAgentOccurrenceState {
            task_id: codec::task_id(&task)?,
            occurrence_id: codec::occurrence_id(&occurrence)?,
            prompt: row.get(2)?,
            prompt_revision: codec::digest(&prompt_revision)?,
            model_reference: row.get(4)?,
            stage: codec::parse_stage(&stage)?,
            revision: codec::revision(revision)?,
            attempt: u16::try_from(attempt).map_err(|_| corrupt("scheduled attempt"))?,
            attempt_id: attempt_id.as_deref().map(codec::attempt_id).transpose()?,
            request_id: request_id.as_deref().map(codec::request_id).transpose()?,
            provider_operation_id: row.get(10)?,
            not_before: row
                .get::<_, Option<i64>>(11)?
                .map(UnixTimestampMilliseconds::new),
            artifact_id: artifact_id.as_deref().map(codec::artifact_id).transpose()?,
            output_digest: output_digest.as_deref().map(codec::digest).transpose()?,
            failure: codec::parse_failure(row.get(14)?, row.get(15)?, row.get(16)?, row.get(17)?)?,
            updated_at: UnixTimestampMilliseconds::new(row.get(18)?),
        })
    })())
}

fn decode_request(
    row: &Row<'_>,
) -> rusqlite::Result<Result<ScheduledAgentHostRequestRecord, StorageError>> {
    let request: Vec<u8> = row.get(0)?;
    let command: Vec<u8> = row.get(1)?;
    let cancellation: Vec<u8> = row.get(2)?;
    let revision: i64 = row.get(3)?;
    let occurrence: Vec<u8> = row.get(5)?;
    let attempt: Vec<u8> = row.get(6)?;
    let prompt_revision: Vec<u8> = row.get(7)?;
    let sequence: Option<i64> = row.get(10)?;
    Ok((|| {
        Ok(ScheduledAgentHostRequestRecord {
            request_id: codec::request_id(&request)?,
            command_id: codec::command_id(&command)?,
            cancellation_id: codec::cancellation_id(&cancellation)?,
            issued_revision: codec::revision(revision)?,
            deadline_at: UnixTimestampMilliseconds::new(row.get(4)?),
            execution: ScheduledAgentExecutionRequest {
                occurrence_id: codec::occurrence_id(&occurrence)?,
                attempt_id: codec::attempt_id(&attempt)?,
                prompt_revision: codec::digest(&prompt_revision)?,
                prompt: row.get(8)?,
                model_reference: row.get(9)?,
                context: Vec::new(),
                maximum_output_bytes: pod0_application::MAX_SCHEDULED_AGENT_OUTPUT_EXCERPT_BYTES
                    as u64,
            },
            last_sequence_number: sequence
                .map(|value| u64::try_from(value).map_err(|_| corrupt("scheduled sequence")))
                .transpose()?,
        })
    })())
}

fn collect<T>(
    rows: rusqlite::MappedRows<
        '_,
        impl FnMut(&Row<'_>) -> rusqlite::Result<Result<T, StorageError>>,
    >,
    operation: &'static str,
) -> Result<Vec<T>, StorageError> {
    rows.map(|row| row.map_err(|error| StorageError::sqlite(operation, error))?)
        .collect()
}

fn page_tasks(mut items: Vec<ScheduledTaskDefinition>, limit: usize) -> ScheduledTaskPage {
    let has_more = items.len() > limit;
    items.truncate(limit);
    ScheduledTaskPage { items, has_more }
}

fn corrupt(_: &'static str) -> StorageError {
    StorageError::CorruptSchema {
        detail: "scheduled numeric value is malformed",
    }
}
