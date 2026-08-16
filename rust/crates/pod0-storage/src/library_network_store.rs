use pod0_domain::{CancellationId, CommandId, HostRequestId, StateRevision};
use rusqlite::{OptionalExtension, params};

use crate::{
    LibraryNetworkWorkflowRecord, LibraryStore, StorageError, StoredLibraryNetworkResult,
    StoredLibraryNetworkStage,
};

impl LibraryStore {
    pub fn admit_library_network(
        &self,
        input: crate::LibraryNetworkAdmissionInput,
    ) -> Result<LibraryNetworkWorkflowRecord, StorageError> {
        crate::transition_commit_library_network_admission::commit(self.path(), input.clone())?;
        self.library_network_workflow(input.command_id)?
            .ok_or(StorageError::EntityNotFound)
    }

    pub fn library_network_workflow(
        &self,
        command_id: CommandId,
    ) -> Result<Option<LibraryNetworkWorkflowRecord>, StorageError> {
        self.read(|connection| read_workflow(connection, command_id))
    }

    pub fn library_network_workflows(
        &self,
        maximum_count: u16,
    ) -> Result<Vec<LibraryNetworkWorkflowRecord>, StorageError> {
        self.read(|connection| {
            let mut statement = connection
                .prepare(
                    "SELECT command_id,cancellation_id,command_fingerprint,intent_json,stage,revision,\
                     pending_request_id,pending_step_json,result_json,failure_code,created_at_ms,updated_at_ms \
                     FROM pod0_library_network_workflows ORDER BY updated_at_ms DESC LIMIT ?1",
                )
                .map_err(|error| StorageError::sqlite("prepare library network snapshot", error))?;
            let rows = statement
                .query_map([i64::from(maximum_count.clamp(1, 200))], decode_row)
                .map_err(|error| StorageError::sqlite("query library network snapshot", error))?;
            rows.map(|row| {
                row.map_err(|error| StorageError::sqlite("decode library network snapshot", error))?
                    .try_into()
            })
            .collect()
        })
    }
}

pub(crate) fn read_workflow(
    connection: &rusqlite::Connection,
    command_id: CommandId,
) -> Result<Option<LibraryNetworkWorkflowRecord>, StorageError> {
    connection
        .query_row(
            "SELECT command_id,cancellation_id,command_fingerprint,intent_json,stage,revision,\
             pending_request_id,pending_step_json,result_json,failure_code,created_at_ms,updated_at_ms \
             FROM pod0_library_network_workflows WHERE command_id=?1",
            [command_id.into_bytes().as_slice()],
            decode_row,
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read library network workflow", error))?
        .map(TryInto::try_into)
        .transpose()
}

type RawRow = (
    Vec<u8>,
    Vec<u8>,
    String,
    String,
    String,
    i64,
    Option<Vec<u8>>,
    Option<String>,
    Option<String>,
    Option<String>,
    i64,
    i64,
);

fn decode_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<RawRow> {
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
    ))
}

impl TryFrom<RawRow> for LibraryNetworkWorkflowRecord {
    type Error = StorageError;

    fn try_from(row: RawRow) -> Result<Self, Self::Error> {
        Ok(Self {
            command_id: CommandId::from_bytes(id(&row.0)?),
            cancellation_id: CancellationId::from_bytes(id(&row.1)?),
            command_fingerprint: row.2,
            intent: json(&row.3)?,
            stage: StoredLibraryNetworkStage::parse(&row.4).ok_or(StorageError::InvalidActivity)?,
            revision: StateRevision::new(
                u64::try_from(row.5).map_err(|_| StorageError::InvalidActivity)?,
            ),
            pending_request_id: row
                .6
                .map(|value| id(&value).map(HostRequestId::from_bytes))
                .transpose()?,
            pending_step: row.7.as_deref().map(json).transpose()?,
            result: row.8.as_deref().map(json).transpose()?,
            failure_code: row.9,
            created_at_ms: row.10,
            updated_at_ms: row.11,
        })
    }
}

pub(crate) fn serialize<T: serde::Serialize>(value: &T) -> Result<String, StorageError> {
    serde_json::to_string(value).map_err(|_| StorageError::InvalidActivity)
}

fn json<T: serde::de::DeserializeOwned>(value: &str) -> Result<T, StorageError> {
    serde_json::from_str(value).map_err(|_| StorageError::InvalidActivity)
}

fn id(value: &[u8]) -> Result<[u8; 16], StorageError> {
    value.try_into().map_err(|_| StorageError::InvalidActivity)
}

pub(crate) fn update_result(
    transaction: &rusqlite::Transaction<'_>,
    command_id: CommandId,
    stage: StoredLibraryNetworkStage,
    revision: StateRevision,
    result: Option<&StoredLibraryNetworkResult>,
    failure: Option<&str>,
    now_ms: i64,
) -> Result<(), StorageError> {
    transaction
        .execute(
            "UPDATE pod0_library_network_workflows SET stage=?1,revision=?2,pending_request_id=NULL,\
             pending_step_json=NULL,result_json=?3,failure_code=?4,updated_at_ms=?5 WHERE command_id=?6",
            params![stage.wire(), i64::try_from(revision.value).map_err(|_| StorageError::InvalidActivity)?,
                result.map(serialize).transpose()?, failure, now_ms, command_id.into_bytes().as_slice()],
        )
        .map_err(|error| StorageError::sqlite("finish library network workflow", error))?;
    Ok(())
}
