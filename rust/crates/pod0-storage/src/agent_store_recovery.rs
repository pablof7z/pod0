use pod0_application::{AgentTurnState, MAX_AGENT_PROJECTION_MESSAGES};
use pod0_domain::AgentTurnId;
use rusqlite::Connection;

use crate::agent_store::read_turn;
use crate::{AgentStore, StorageError};

impl AgentStore {
    pub fn recoverable_turns(
        &self,
        maximum_count: u16,
    ) -> Result<Vec<AgentTurnState>, StorageError> {
        self.read(|connection| read_recoverable(connection, maximum_count))
    }
}

fn read_recoverable(
    connection: &Connection,
    maximum_count: u16,
) -> Result<Vec<AgentTurnState>, StorageError> {
    let limit = i64::from(maximum_count.clamp(1, MAX_AGENT_PROJECTION_MESSAGES as u16));
    let mut statement = connection
        .prepare(
            "SELECT turn_id FROM pod0_agent_turns WHERE stage IN ('awaiting_model','approval_required','authorized','executing') ORDER BY updated_at_ms,turn_id LIMIT ?1",
        )
        .map_err(|error| StorageError::sqlite("prepare recoverable agent turns", error))?;
    let rows = statement
        .query_map([limit], |row| row.get::<_, Vec<u8>>(0))
        .map_err(|error| StorageError::sqlite("read recoverable agent turns", error))?;
    rows.map(|row| {
        let bytes: [u8; 16] = row
            .map_err(|error| StorageError::sqlite("decode recoverable agent turn", error))?
            .try_into()
            .map_err(|_| StorageError::CorruptSchema {
                detail: "agent turn id is malformed",
            })?;
        read_turn(connection, AgentTurnId::from_bytes(bytes))?
            .ok_or(StorageError::AgentTurnNotFound)
    })
    .collect()
}
