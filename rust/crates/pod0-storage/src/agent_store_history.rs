use pod0_application::{
    AgentConversationSummaryProjection, AgentMessageRole, MAX_AGENT_CONVERSATION_PREVIEW_BYTES,
    MAX_AGENT_CONVERSATION_SUMMARIES, MAX_AGENT_CONVERSATION_TITLE_BYTES,
    bounded_agent_summary_text,
};
use pod0_domain::{AgentTurnId, ConversationId, UnixTimestampMilliseconds};
use rusqlite::{Connection, OptionalExtension, params};

use crate::agent_store::read_turn;
use crate::{AgentConversationPage, AgentStore, StorageError};

type ConversationRow = (Vec<u8>, i64, i64, i64);

impl AgentStore {
    pub fn conversation_page(
        &self,
        offset: u32,
        max_items: u16,
    ) -> Result<AgentConversationPage, StorageError> {
        self.read(|connection| read_conversation_page(connection, offset, max_items))
    }
}

fn read_conversation_page(
    connection: &Connection,
    offset: u32,
    max_items: u16,
) -> Result<AgentConversationPage, StorageError> {
    let limit = usize::from(max_items.clamp(1, MAX_AGENT_CONVERSATION_SUMMARIES));
    let sql_limit = i64::try_from(limit + 1).map_err(|_| StorageError::InvalidAgentState)?;
    let mut statement = connection
        .prepare(
            "SELECT conversation_id,MIN(created_at_ms),MAX(updated_at_ms),COUNT(*) \
             FROM pod0_agent_turns GROUP BY conversation_id \
             ORDER BY MAX(updated_at_ms) DESC,conversation_id DESC LIMIT ?1 OFFSET ?2",
        )
        .map_err(|error| StorageError::sqlite("prepare agent conversation page", error))?;
    let rows = statement
        .query_map(params![sql_limit, i64::from(offset)], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })
        .map_err(|error| StorageError::sqlite("read agent conversation page", error))?;
    let mut rows = rows
        .collect::<Result<Vec<ConversationRow>, _>>()
        .map_err(|error| StorageError::sqlite("decode agent conversation page", error))?;
    let has_more = rows.len() > limit;
    rows.truncate(limit);
    let items = rows
        .into_iter()
        .map(|(conversation, created_at, updated_at, count)| {
            summary(connection, conversation, created_at, updated_at, count)
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(AgentConversationPage { items, has_more })
}

fn summary(
    connection: &Connection,
    conversation: Vec<u8>,
    created_at: i64,
    updated_at: i64,
    count: i64,
) -> Result<AgentConversationSummaryProjection, StorageError> {
    let conversation_id = decode_conversation_id(conversation)?;
    let first = boundary_turn(connection, conversation_id, true)?;
    let latest = boundary_turn(connection, conversation_id, false)?;
    let stored_title: Option<String> = connection
        .query_row(
            "SELECT title FROM pod0_agent_conversation_metadata WHERE conversation_id=?1",
            [conversation_id.into_bytes().as_slice()],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read agent conversation metadata", error))?;
    let title = stored_title
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| {
            first
                .messages
                .iter()
                .find(|message| message.role == AgentMessageRole::User)
                .map_or("", |message| message.content.trim())
        });
    let preview = latest
        .messages
        .iter()
        .rev()
        .find(|message| {
            message.role != AgentMessageRole::Tool && !message.content.trim().is_empty()
        })
        .map_or("", |message| message.content.trim());
    Ok(AgentConversationSummaryProjection {
        conversation_id,
        title: bounded_agent_summary_text(title, MAX_AGENT_CONVERSATION_TITLE_BYTES),
        preview: bounded_agent_summary_text(preview, MAX_AGENT_CONVERSATION_PREVIEW_BYTES),
        turn_count: u32::try_from(count).map_err(|_| StorageError::InvalidAgentState)?,
        latest_stage: latest.stage,
        created_at: UnixTimestampMilliseconds::new(created_at),
        updated_at: UnixTimestampMilliseconds::new(updated_at),
    })
}

fn boundary_turn(
    connection: &Connection,
    conversation_id: ConversationId,
    first: bool,
) -> Result<pod0_application::AgentTurnProjection, StorageError> {
    let ordering = if first {
        "created_at_ms ASC,rowid ASC"
    } else {
        "updated_at_ms DESC,rowid DESC"
    };
    let sql = format!(
        "SELECT turn_id FROM pod0_agent_turns WHERE conversation_id=?1 ORDER BY {ordering} LIMIT 1"
    );
    let bytes: Vec<u8> = connection
        .query_row(&sql, [conversation_id.into_bytes().as_slice()], |row| {
            row.get(0)
        })
        .map_err(|error| StorageError::sqlite("read agent conversation boundary", error))?;
    let bytes: [u8; 16] = bytes.try_into().map_err(|_| StorageError::CorruptSchema {
        detail: "agent turn id is malformed",
    })?;
    read_turn(connection, AgentTurnId::from_bytes(bytes))?
        .map(|state| state.projection())
        .ok_or(StorageError::AgentTurnNotFound)
}

fn decode_conversation_id(bytes: Vec<u8>) -> Result<ConversationId, StorageError> {
    let bytes: [u8; 16] = bytes.try_into().map_err(|_| StorageError::CorruptSchema {
        detail: "agent conversation id is malformed",
    })?;
    Ok(ConversationId::from_bytes(bytes))
}
