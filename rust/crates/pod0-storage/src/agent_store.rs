use std::path::{Path, PathBuf};

use pod0_application::{AgentTurnProjection, AgentTurnState, MAX_AGENT_PROJECTION_MESSAGES};
use pod0_domain::{AgentTurnId, ConversationId};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};

use crate::agent_store_codec::{
    AGENT_STATE_SCHEMA_VERSION, decode_state, encode_state, stage_code,
};
use crate::migration_db::{configure, open_connection, user_version, validate_open_database};
use crate::{
    AgentAuditKind, AgentCommandContext, AgentMutationOutcome, AgentTurnMutation, AgentTurnPage,
    CURRENT_SCHEMA_VERSION, StorageError,
};

#[derive(Clone, Debug)]
pub struct AgentStore {
    path: PathBuf,
}

type StoredAgentTurnRow = (Vec<u8>, i64, String, u32, Vec<u8>, Vec<u8>);

impl AgentStore {
    pub fn open(path: &Path) -> Result<Self, StorageError> {
        let connection = open_current(path, true)?;
        drop(connection);
        Ok(Self {
            path: path.to_owned(),
        })
    }

    pub fn turn(&self, turn_id: AgentTurnId) -> Result<Option<AgentTurnState>, StorageError> {
        self.read(|connection| read_turn(connection, turn_id))
    }

    pub fn turn_page(
        &self,
        conversation_id: ConversationId,
        offset: u32,
        max_items: u16,
    ) -> Result<AgentTurnPage, StorageError> {
        self.read(|connection| read_page(connection, conversation_id, offset, max_items))
    }

    pub fn start_turn(
        &self,
        context: AgentCommandContext,
        state: &AgentTurnState,
    ) -> Result<AgentMutationOutcome, StorageError> {
        self.write(|transaction| {
            persist(transaction, context, None, AgentAuditKind::Started, state)
        })
    }

    pub fn update_turn(
        &self,
        context: AgentCommandContext,
        mutation: AgentTurnMutation,
        state: &AgentTurnState,
    ) -> Result<AgentMutationOutcome, StorageError> {
        self.write(|transaction| {
            persist(
                transaction,
                context,
                Some(mutation.expected_revision),
                mutation.audit_kind,
                state,
            )
        })
    }

    pub(crate) fn read<T>(
        &self,
        operation: impl FnOnce(&Connection) -> Result<T, StorageError>,
    ) -> Result<T, StorageError> {
        let connection = open_current(&self.path, true)?;
        operation(&connection)
    }

    fn write<T>(
        &self,
        operation: impl FnOnce(&Transaction<'_>) -> Result<T, StorageError>,
    ) -> Result<T, StorageError> {
        let mut connection = open_current(&self.path, false)?;
        configure(&connection)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| StorageError::sqlite("begin agent command", error))?;
        let output = operation(&transaction)?;
        transaction
            .commit()
            .map_err(|error| StorageError::sqlite("commit agent command", error))?;
        Ok(output)
    }
}

fn persist(
    transaction: &Transaction<'_>,
    context: AgentCommandContext,
    expected_revision: Option<pod0_domain::StateRevision>,
    audit_kind: AgentAuditKind,
    state: &AgentTurnState,
) -> Result<AgentMutationOutcome, StorageError> {
    let projection = state.projection();
    if let Some(duplicate) = command_receipt(transaction, context, projection.turn_id)? {
        return Ok(AgentMutationOutcome::Duplicate(duplicate));
    }
    let current = read_turn(transaction, projection.turn_id)?;
    match (current.as_ref(), expected_revision) {
        (None, None) => {}
        (Some(current), Some(expected))
            if current.projection().revision == expected
                && current.projection().conversation_id == projection.conversation_id
                && projection.revision.value > expected.value => {}
        (None, Some(_)) => return Err(StorageError::AgentTurnNotFound),
        _ => return Err(StorageError::AgentTurnConflict),
    }
    let (state_json, state_digest) = encode_state(state)?;
    let stored_revision =
        i64::try_from(projection.revision.value).map_err(|_| StorageError::InvalidAgentState)?;
    let created_at = if current.is_some() {
        read_created_at(transaction, projection.turn_id)?
    } else {
        projection.updated_at.value
    };
    transaction
        .execute(
            "INSERT INTO pod0_agent_turns(turn_id,conversation_id,state_revision,stage,state_schema_version,state_json,state_digest,created_at_ms,updated_at_ms) \
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9) ON CONFLICT(turn_id) DO UPDATE SET \
             state_revision=excluded.state_revision,stage=excluded.stage,state_schema_version=excluded.state_schema_version, \
             state_json=excluded.state_json,state_digest=excluded.state_digest,updated_at_ms=excluded.updated_at_ms",
            params![
                projection.turn_id.into_bytes().as_slice(),
                projection.conversation_id.into_bytes().as_slice(),
                stored_revision,
                stage_code(projection.stage),
                AGENT_STATE_SCHEMA_VERSION,
                state_json,
                state_digest.as_slice(),
                created_at,
                projection.updated_at.value,
            ],
        )
        .map_err(|error| StorageError::sqlite("write agent turn", error))?;
    transaction
        .execute(
            "INSERT INTO pod0_agent_audit(turn_id,turn_revision,event_kind,state_digest,observed_at_ms) VALUES(?1,?2,?3,?4,?5)",
            params![projection.turn_id.into_bytes().as_slice(), stored_revision, audit_kind.code(), state_digest.as_slice(), context.observed_at.value],
        )
        .map_err(|error| StorageError::sqlite("write agent audit", error))?;
    transaction
        .execute(
            "INSERT INTO pod0_agent_command_receipts(command_id,command_fingerprint,turn_id,applied_revision,completed_at_ms) VALUES(?1,?2,?3,?4,?5)",
            params![context.command_id.into_bytes().as_slice(), context.command_fingerprint.as_slice(), projection.turn_id.into_bytes().as_slice(), stored_revision, context.observed_at.value],
        )
        .map_err(|error| StorageError::sqlite("write agent command receipt", error))?;
    Ok(AgentMutationOutcome::Applied(state.clone()))
}

fn read_created_at(connection: &Connection, turn_id: AgentTurnId) -> Result<i64, StorageError> {
    connection
        .query_row(
            "SELECT created_at_ms FROM pod0_agent_turns WHERE turn_id=?1",
            [turn_id.into_bytes().as_slice()],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("read agent turn creation time", error))
}

fn command_receipt(
    connection: &Connection,
    context: AgentCommandContext,
    turn_id: AgentTurnId,
) -> Result<Option<AgentTurnState>, StorageError> {
    let row: Option<(Vec<u8>, Vec<u8>)> = connection
        .query_row(
            "SELECT command_fingerprint,turn_id FROM pod0_agent_command_receipts WHERE command_id=?1",
            [context.command_id.into_bytes().as_slice()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read agent command receipt", error))?;
    match row {
        None => Ok(None),
        Some((fingerprint, stored_turn))
            if fingerprint == context.command_fingerprint
                && stored_turn == turn_id.into_bytes().as_slice() =>
        {
            read_turn(connection, turn_id)
        }
        Some(_) => Err(StorageError::AgentCommandConflict),
    }
}

pub(crate) fn read_turn(
    connection: &Connection,
    turn_id: AgentTurnId,
) -> Result<Option<AgentTurnState>, StorageError> {
    let row: Option<StoredAgentTurnRow> = connection
        .query_row(
            "SELECT conversation_id,state_revision,stage,state_schema_version,state_json,state_digest FROM pod0_agent_turns WHERE turn_id=?1",
            [turn_id.into_bytes().as_slice()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?)),
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read agent turn", error))?;
    row.map(|(conversation, revision, stage, schema, bytes, digest)| {
        if schema != AGENT_STATE_SCHEMA_VERSION {
            return Err(StorageError::CorruptSchema {
                detail: "agent state schema is unsupported",
            });
        }
        let revision = u64::try_from(revision).map_err(|_| StorageError::CorruptSchema {
            detail: "agent state revision is malformed",
        })?;
        let state = decode_state(&bytes, &digest)?;
        let projection = state.projection();
        if projection.turn_id != turn_id
            || projection.conversation_id.into_bytes().as_slice() != conversation
            || projection.revision.value != revision
            || stage_code(projection.stage) != stage
        {
            return Err(StorageError::CorruptSchema {
                detail: "agent state columns disagree with payload",
            });
        }
        Ok(state)
    })
    .transpose()
}

fn read_page(
    connection: &Connection,
    conversation_id: ConversationId,
    offset: u32,
    max_items: u16,
) -> Result<AgentTurnPage, StorageError> {
    let limit = usize::from(max_items.clamp(1, MAX_AGENT_PROJECTION_MESSAGES as u16));
    let sql_limit = i64::try_from(limit + 1).map_err(|_| StorageError::InvalidAgentState)?;
    let sql_offset = i64::from(offset);
    let mut statement = connection
        .prepare("SELECT turn_id FROM pod0_agent_turns WHERE conversation_id=?1 ORDER BY created_at_ms DESC,rowid DESC LIMIT ?2 OFFSET ?3")
        .map_err(|error| StorageError::sqlite("prepare agent turn page", error))?;
    let rows = statement
        .query_map(
            params![
                conversation_id.into_bytes().as_slice(),
                sql_limit,
                sql_offset
            ],
            |row| row.get::<_, Vec<u8>>(0),
        )
        .map_err(|error| StorageError::sqlite("read agent turn page", error))?;
    let mut ids = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| StorageError::sqlite("decode agent turn page", error))?;
    let has_more = ids.len() > limit;
    ids.truncate(limit);
    let items = ids
        .into_iter()
        .map(|bytes| {
            let bytes: [u8; 16] = bytes.try_into().map_err(|_| StorageError::CorruptSchema {
                detail: "agent turn id is malformed",
            })?;
            read_turn(connection, AgentTurnId::from_bytes(bytes))?
                .map(|state| state.projection())
                .ok_or(StorageError::AgentTurnNotFound)
        })
        .collect::<Result<Vec<AgentTurnProjection>, StorageError>>()?;
    Ok(AgentTurnPage { items, has_more })
}

fn open_current(path: &Path, read_only: bool) -> Result<Connection, StorageError> {
    let connection = open_connection(path, read_only)?;
    let version = user_version(&connection)?;
    validate_open_database(&connection, version)?;
    if version != CURRENT_SCHEMA_VERSION {
        return Err(StorageError::CorruptSchema {
            detail: "agent store schema is not current",
        });
    }
    Ok(connection)
}
