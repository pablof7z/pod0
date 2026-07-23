use pod0_domain::{
    CompiledMemoryRecord, MemoryId, MemoryRecord, MemoryRevision, MemorySource, StateRevision,
    UnixTimestampMilliseconds,
};
use rusqlite::{Connection, OptionalExtension};

use crate::{MemoryCollectionSnapshot, StorageError};

pub(crate) fn require_memories_authoritative(connection: &Connection) -> Result<(), StorageError> {
    let active: i64 = connection
        .query_row(
            "SELECT authority_active FROM pod0_memory_state WHERE singleton=1",
            [],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("read memory authority", error))?;
    if active == 1 {
        Ok(())
    } else {
        Err(StorageError::CutoverNotAuthoritative)
    }
}

pub(crate) fn read_memory_snapshot(
    connection: &Connection,
) -> Result<MemoryCollectionSnapshot, StorageError> {
    let (revision, active): (i64, i64) = connection
        .query_row(
            "SELECT collection_revision,authority_active FROM pod0_memory_state WHERE singleton=1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|error| StorageError::sqlite("read memory state", error))?;
    if active == 0 {
        return Ok(MemoryCollectionSnapshot {
            revision: StateRevision::INITIAL,
            memories: Vec::new(),
            compiled: None,
        });
    }
    if active != 1 {
        return Err(StorageError::CorruptSchema {
            detail: "memory authority state is malformed",
        });
    }
    let mut statement = connection
        .prepare(
            "SELECT memory_id,memory_revision,content,source_code,created_at_ms,updated_at_ms,deleted \
             FROM pod0_memories ORDER BY created_at_ms DESC,memory_id ASC",
        )
        .map_err(|error| StorageError::sqlite("prepare memory projection", error))?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, Vec<u8>>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, i64>(6)?,
            ))
        })
        .map_err(|error| StorageError::sqlite("read memory projection", error))?;
    let memories = rows
        .map(|row| {
            let row = row.map_err(|error| StorageError::sqlite("decode memory row", error))?;
            Ok(MemoryRecord {
                memory_id: memory_id(&row.0)?,
                revision: MemoryRevision::new(u64::try_from(row.1).map_err(|_| {
                    StorageError::CorruptSchema {
                        detail: "memory revision is malformed",
                    }
                })?),
                content: row.2,
                source: decode_source(row.3)?,
                created_at: UnixTimestampMilliseconds::new(row.4),
                updated_at: UnixTimestampMilliseconds::new(row.5),
                deleted: decode_bool(row.6)?,
            })
        })
        .collect::<Result<Vec<_>, StorageError>>()?;
    Ok(MemoryCollectionSnapshot {
        revision: StateRevision::new(u64::try_from(revision).map_err(|_| {
            StorageError::CorruptSchema {
                detail: "memory collection revision is malformed",
            }
        })?),
        memories,
        compiled: read_compiled_memory(connection)?,
    })
}

fn read_compiled_memory(
    connection: &Connection,
) -> Result<Option<CompiledMemoryRecord>, StorageError> {
    let row: Option<(String, i64)> = connection
        .query_row(
            "SELECT text,compiled_at_ms FROM pod0_compiled_memory WHERE singleton=1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read compiled memory", error))?;
    let Some((text, compiled_at)) = row else {
        return Ok(None);
    };
    let mut statement = connection
        .prepare(
            "SELECT memory_id FROM pod0_compiled_memory_sources \
             WHERE singleton=1 ORDER BY sort_order",
        )
        .map_err(|error| StorageError::sqlite("prepare compiled memory sources", error))?;
    let rows = statement
        .query_map([], |row| row.get::<_, Vec<u8>>(0))
        .map_err(|error| StorageError::sqlite("read compiled memory sources", error))?;
    let source_memory_ids = rows
        .map(|row| {
            memory_id(
                &row.map_err(|error| StorageError::sqlite("decode compiled memory source", error))?,
            )
        })
        .collect::<Result<Vec<_>, StorageError>>()?;
    Ok(Some(CompiledMemoryRecord {
        text,
        compiled_at: UnixTimestampMilliseconds::new(compiled_at),
        source_memory_ids,
    }))
}

pub(crate) fn memory_id(bytes: &[u8]) -> Result<MemoryId, StorageError> {
    let bytes: [u8; 16] = bytes.try_into().map_err(|_| StorageError::CorruptSchema {
        detail: "memory ID is malformed",
    })?;
    Ok(MemoryId::from_bytes(bytes))
}

pub(crate) fn decode_source(value: i64) -> Result<MemorySource, StorageError> {
    match value {
        1 => Ok(MemorySource::Agent),
        2 => Ok(MemorySource::LegacySwift),
        _ => Err(StorageError::CorruptSchema {
            detail: "memory source is malformed",
        }),
    }
}

pub(crate) const fn encode_source(value: MemorySource) -> Result<i64, StorageError> {
    match value {
        MemorySource::Agent => Ok(1),
        MemorySource::LegacySwift => Ok(2),
        MemorySource::Unsupported { .. } => Err(StorageError::InvalidMemory),
    }
}

fn decode_bool(value: i64) -> Result<bool, StorageError> {
    match value {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(StorageError::CorruptSchema {
            detail: "memory deletion state is malformed",
        }),
    }
}
