use pod0_application::DurableInternalCommandRequest;
use pod0_domain::{ActivityCorrelationId, ActivityId, InternalCommandId};
use rusqlite::params;

use crate::{LibraryStore, StorageError};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingInternalCommand {
    pub internal_command_id: InternalCommandId,
    pub authorizing_activity_id: ActivityId,
    pub correlation_id: ActivityCorrelationId,
    pub request: DurableInternalCommandRequest,
    pub committed_at_ms: i64,
}

impl LibraryStore {
    pub fn record_internal_command_disposition(
        &self,
        command: PendingInternalCommand,
        state_revision: pod0_domain::StateRevision,
        disposition: pod0_application::RequestDisposition,
        observed_at: pod0_domain::UnixTimestampMilliseconds,
    ) -> Result<crate::CommitReceipt, StorageError> {
        crate::transition_commit::commit_internal_command_disposition(
            self.path(),
            command,
            state_revision,
            disposition,
            observed_at,
        )
    }

    pub fn pending_internal_commands(
        &self,
        maximum_count: u16,
    ) -> Result<Vec<PendingInternalCommand>, StorageError> {
        let limit = i64::from(maximum_count.clamp(1, 100));
        self.read(|connection| {
            let mut statement = connection
                .prepare(
                    "SELECT internal_command_id,authorizing_activity_id,correlation_id,\
                     command_json,committed_at_ms FROM pod0_internal_command_intents \
                     WHERE state_code=1 ORDER BY committed_at_ms,internal_command_id LIMIT ?1",
                )
                .map_err(|error| StorageError::sqlite("prepare internal commands", error))?;
            statement
                .query_map(params![limit], |row| {
                    Ok((
                        row.get::<_, Vec<u8>>(0)?,
                        row.get::<_, Vec<u8>>(1)?,
                        row.get::<_, Vec<u8>>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, i64>(4)?,
                    ))
                })
                .map_err(|error| StorageError::sqlite("read internal commands", error))?
                .map(|row| {
                    let (command, activity, correlation, payload, committed_at_ms) = row
                        .map_err(|error| StorageError::sqlite("decode internal command", error))?;
                    Ok(PendingInternalCommand {
                        internal_command_id: InternalCommandId::from_bytes(id(&command)?),
                        authorizing_activity_id: ActivityId::from_bytes(id(&activity)?),
                        correlation_id: ActivityCorrelationId::from_bytes(id(&correlation)?),
                        request: serde_json::from_str(&payload)
                            .map_err(|_| StorageError::InvalidActivity)?,
                        committed_at_ms,
                    })
                })
                .collect()
        })
    }

    pub fn pending_download_finalization_commands(
        &self,
        maximum_count: u16,
    ) -> Result<Vec<PendingInternalCommand>, StorageError> {
        let limit = i64::from(maximum_count.clamp(1, 100));
        self.read(|connection| {
            let mut statement = connection
                .prepare(
                    "SELECT internal_command_id,authorizing_activity_id,correlation_id,\
                     command_json,committed_at_ms FROM pod0_internal_command_intents \
                     WHERE state_code=1 AND target_domain_code=3 \
                     AND json_type(command_json,'$.kind.FinalizeDownloadArtifact')='object' \
                     ORDER BY committed_at_ms,internal_command_id LIMIT ?1",
                )
                .map_err(|error| {
                    StorageError::sqlite("prepare download finalization commands", error)
                })?;
            statement
                .query_map(params![limit], decode_command_row)
                .map_err(|error| {
                    StorageError::sqlite("read download finalization commands", error)
                })?
                .map(|row| {
                    let (command, activity, correlation, payload, committed_at_ms) =
                        row.map_err(|error| {
                            StorageError::sqlite("decode download finalization command", error)
                        })?;
                    pending_command(command, activity, correlation, payload, committed_at_ms)
                })
                .collect()
        })
    }
}

fn decode_command_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<(Vec<u8>, Vec<u8>, Vec<u8>, String, i64)> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
    ))
}

fn pending_command(
    command: Vec<u8>,
    activity: Vec<u8>,
    correlation: Vec<u8>,
    payload: String,
    committed_at_ms: i64,
) -> Result<PendingInternalCommand, StorageError> {
    Ok(PendingInternalCommand {
        internal_command_id: InternalCommandId::from_bytes(id(&command)?),
        authorizing_activity_id: ActivityId::from_bytes(id(&activity)?),
        correlation_id: ActivityCorrelationId::from_bytes(id(&correlation)?),
        request: serde_json::from_str(&payload).map_err(|_| StorageError::InvalidActivity)?,
        committed_at_ms,
    })
}

fn id(value: &[u8]) -> Result<[u8; 16], StorageError> {
    value.try_into().map_err(|_| StorageError::InvalidActivity)
}
