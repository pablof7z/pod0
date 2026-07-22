use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior};

use crate::migration_db::{configure, open_connection, user_version, validate_open_database};
use crate::{CURRENT_SCHEMA_VERSION, ScheduledAgentAuthorityState, StorageError};

#[derive(Clone, Debug)]
pub struct ScheduledAgentStore {
    path: PathBuf,
}

impl ScheduledAgentStore {
    pub fn open_authoritative(path: &Path) -> Result<Self, StorageError> {
        let connection = open_current(path, true)?;
        require_authoritative(&connection)?;
        Ok(Self {
            path: path.to_owned(),
        })
    }

    pub fn authority(&self) -> Result<ScheduledAgentAuthorityState, StorageError> {
        self.read(read_authority)
    }

    pub fn task(
        &self,
        task_id: pod0_domain::ScheduledTaskId,
    ) -> Result<Option<pod0_application::ScheduledTaskDefinition>, StorageError> {
        self.read(|connection| {
            crate::scheduled_agent_store_read::read_task(connection, task_id, false)
        })
    }

    pub fn occurrence(
        &self,
        occurrence_id: pod0_domain::ScheduledOccurrenceId,
    ) -> Result<Option<pod0_application::ScheduledAgentOccurrenceState>, StorageError> {
        self.read(|connection| {
            crate::scheduled_agent_store_read::read_occurrence(connection, occurrence_id)
        })
    }

    pub fn task_page(
        &self,
        offset: u32,
        max_items: u16,
    ) -> Result<crate::ScheduledTaskPage, StorageError> {
        self.read(|connection| {
            crate::scheduled_agent_store_read::task_page(connection, offset, max_items)
        })
    }

    pub fn occurrence_page(
        &self,
        task_id: Option<pod0_domain::ScheduledTaskId>,
        offset: u32,
        max_items: u16,
    ) -> Result<crate::ScheduledOccurrencePage, StorageError> {
        self.read(|connection| {
            crate::scheduled_agent_store_read::occurrence_page(
                connection, task_id, offset, max_items,
            )
        })
    }

    pub fn pending_host_requests(
        &self,
        max_items: u16,
    ) -> Result<Vec<crate::ScheduledAgentHostRequestRecord>, StorageError> {
        self.read(|connection| {
            crate::scheduled_agent_store_read::pending_requests(connection, None, max_items)
        })
    }

    pub(crate) fn read<T>(
        &self,
        operation: impl FnOnce(&Connection) -> Result<T, StorageError>,
    ) -> Result<T, StorageError> {
        let connection = open_current(&self.path, true)?;
        require_authoritative(&connection)?;
        operation(&connection)
    }

    pub(crate) fn write<T>(
        &self,
        operation: impl FnOnce(&Transaction<'_>) -> Result<T, StorageError>,
    ) -> Result<T, StorageError> {
        let mut connection = open_current(&self.path, false)?;
        configure(&connection)?;
        require_authoritative(&connection)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| StorageError::sqlite("begin scheduled-agent command", error))?;
        let output = operation(&transaction)?;
        transaction
            .commit()
            .map_err(|error| StorageError::sqlite("commit scheduled-agent command", error))?;
        Ok(output)
    }
}

pub fn scheduled_agent_store_is_authoritative(path: &Path) -> Result<bool, StorageError> {
    let connection = open_current(path, true)?;
    Ok(read_authority(&connection)?.is_authoritative())
}

pub(crate) fn read_authority(
    connection: &Connection,
) -> Result<ScheduledAgentAuthorityState, StorageError> {
    let row: Option<(String, Option<i64>)> = connection
        .query_row(
            "SELECT state,source_generation FROM pod0_scheduled_agent_authority WHERE singleton=1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read scheduled-agent authority", error))?;
    match row {
        Some((state, None)) if state == "inactive" => Ok(ScheduledAgentAuthorityState::Inactive),
        Some((state, Some(generation))) if state == "authoritative" => {
            let source_generation =
                u64::try_from(generation).map_err(|_| StorageError::CorruptSchema {
                    detail: "scheduled-agent authority generation is malformed",
                })?;
            Ok(ScheduledAgentAuthorityState::Authoritative { source_generation })
        }
        _ => Err(StorageError::CorruptSchema {
            detail: "scheduled-agent authority is malformed",
        }),
    }
}

pub(crate) fn require_authoritative(connection: &Connection) -> Result<(), StorageError> {
    if read_authority(connection)?.is_authoritative() {
        Ok(())
    } else {
        Err(StorageError::CutoverNotAuthoritative)
    }
}

fn open_current(path: &Path, read_only: bool) -> Result<Connection, StorageError> {
    let connection = open_connection(path, read_only)?;
    let version = user_version(&connection)?;
    validate_open_database(&connection, version)?;
    if version != CURRENT_SCHEMA_VERSION {
        return Err(StorageError::CorruptSchema {
            detail: "scheduled-agent store schema is not current",
        });
    }
    Ok(connection)
}
