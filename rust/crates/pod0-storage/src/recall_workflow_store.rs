use pod0_application::StoredRecallQueryWorkflow;
use pod0_domain::{CommandId, RecallQueryId};
use rusqlite::{OptionalExtension, params};

use crate::{LibraryStore, StorageError};

impl LibraryStore {
    pub fn recall_query_workflow(
        &self,
        query_id: RecallQueryId,
    ) -> Result<Option<StoredRecallQueryWorkflow>, StorageError> {
        self.read(|connection| read_query(connection, query_id))
    }

    pub fn recall_query_workflows(&self) -> Result<Vec<StoredRecallQueryWorkflow>, StorageError> {
        self.read(|connection| {
            let mut statement = connection.prepare(
                "SELECT command_id,cancellation_id,query_json,revision,stage_json,evidence_json,\
                 failure_json,created_at_ms,updated_at_ms FROM pod0_recall_queries ORDER BY created_at_ms",
            ).map_err(|error| StorageError::sqlite("prepare recall workflows", error))?;
            statement.query_map([], decode)
                .map_err(|error| StorageError::sqlite("read recall workflows", error))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| StorageError::sqlite("decode recall workflows", error))
        })
    }

    pub fn start_recall_query(
        &self,
        command_id: CommandId,
        fingerprint: &str,
        cancellation_id: pod0_domain::CancellationId,
        query: pod0_application::RecallQuery,
        initial_stage: pod0_application::RecallStage,
        initial_failure: Option<pod0_application::CoreFailure>,
        observed_at: pod0_domain::UnixTimestampMilliseconds,
    ) -> Result<StoredRecallQueryWorkflow, StorageError> {
        crate::transition_commit::commit_recall_query_start(
            self.path(),
            command_id,
            fingerprint,
            cancellation_id,
            query,
            initial_stage,
            initial_failure,
            observed_at,
        )
    }

    pub fn commit_recall_query_observation(
        &self,
        lease: pod0_application::PersistedEffectLeaseIdentity,
        observation: pod0_application::DurableAgentRecallHostObservation,
        resolution: pod0_application::RecallQueryResolution,
        committed_at: pod0_domain::UnixTimestampMilliseconds,
    ) -> Result<(StoredRecallQueryWorkflow, bool), StorageError> {
        crate::transition_commit::commit_recall_query_observation(
            self.path(),
            lease,
            observation,
            resolution,
            committed_at,
        )
    }
}

pub(crate) fn read_query(
    connection: &rusqlite::Connection,
    query_id: RecallQueryId,
) -> Result<Option<StoredRecallQueryWorkflow>, StorageError> {
    connection
        .query_row(
            "SELECT command_id,cancellation_id,query_json,revision,stage_json,evidence_json,\
         failure_json,created_at_ms,updated_at_ms FROM pod0_recall_queries WHERE query_id=?1",
            [query_id.into_bytes().as_slice()],
            decode,
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read recall query", error))
}

fn decode(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredRecallQueryWorkflow> {
    let command: Vec<u8> = row.get(0)?;
    let cancellation: Vec<u8> = row.get(1)?;
    let query: String = row.get(2)?;
    let revision: i64 = row.get(3)?;
    let stage: String = row.get(4)?;
    let evidence: String = row.get(5)?;
    let failure: Option<String> = row.get(6)?;
    Ok(StoredRecallQueryWorkflow {
        command_id: CommandId::from_bytes(
            command
                .try_into()
                .map_err(|_| rusqlite::Error::InvalidQuery)?,
        ),
        cancellation_id: pod0_domain::CancellationId::from_bytes(
            cancellation
                .try_into()
                .map_err(|_| rusqlite::Error::InvalidQuery)?,
        ),
        query: serde_json::from_str(&query).map_err(|_| rusqlite::Error::InvalidQuery)?,
        revision: pod0_domain::StateRevision::new(
            u64::try_from(revision).map_err(|_| rusqlite::Error::InvalidQuery)?,
        ),
        stage: serde_json::from_str(&stage).map_err(|_| rusqlite::Error::InvalidQuery)?,
        evidence: serde_json::from_str(&evidence).map_err(|_| rusqlite::Error::InvalidQuery)?,
        failure: failure
            .map(|value| serde_json::from_str(&value).map_err(|_| rusqlite::Error::InvalidQuery))
            .transpose()?,
        created_at: pod0_domain::UnixTimestampMilliseconds::new(row.get(7)?),
        updated_at: pod0_domain::UnixTimestampMilliseconds::new(row.get(8)?),
    })
}

pub(crate) fn insert_query(
    transaction: &rusqlite::Transaction<'_>,
    workflow: &StoredRecallQueryWorkflow,
) -> Result<(), StorageError> {
    transaction.execute(
        "INSERT INTO pod0_recall_queries(query_id,command_id,cancellation_id,revision,query_json,\
         stage_json,evidence_json,failure_json,created_at_ms,updated_at_ms) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
        params![workflow.query.query_id.into_bytes().as_slice(), workflow.command_id.into_bytes().as_slice(),
            workflow.cancellation_id.into_bytes().as_slice(), i64::try_from(workflow.revision.value).map_err(|_| StorageError::InvalidActivity)?,
            serde_json::to_string(&workflow.query).map_err(|_| StorageError::InvalidActivity)?,
            serde_json::to_string(&workflow.stage).map_err(|_| StorageError::InvalidActivity)?,
            serde_json::to_string(&workflow.evidence).map_err(|_| StorageError::InvalidActivity)?,
            workflow.failure.as_ref().map(serde_json::to_string).transpose().map_err(|_| StorageError::InvalidActivity)?,
            workflow.created_at.value, workflow.updated_at.value],
    ).map_err(|error| StorageError::sqlite("insert recall query", error))?;
    Ok(())
}

pub(crate) fn update_query(
    transaction: &rusqlite::Transaction<'_>,
    workflow: &StoredRecallQueryWorkflow,
) -> Result<(), StorageError> {
    let changed = transaction.execute(
        "UPDATE pod0_recall_queries SET revision=?1,stage_json=?2,evidence_json=?3,failure_json=?4,updated_at_ms=?5 WHERE query_id=?6",
        params![i64::try_from(workflow.revision.value).map_err(|_| StorageError::InvalidActivity)?,
            serde_json::to_string(&workflow.stage).map_err(|_| StorageError::InvalidActivity)?,
            serde_json::to_string(&workflow.evidence).map_err(|_| StorageError::InvalidActivity)?,
            workflow.failure.as_ref().map(serde_json::to_string).transpose().map_err(|_| StorageError::InvalidActivity)?,
            workflow.updated_at.value, workflow.query.query_id.into_bytes().as_slice()],
    ).map_err(|error| StorageError::sqlite("update recall query", error))?;
    (changed == 1)
        .then_some(())
        .ok_or(StorageError::RevisionConflict)
}
