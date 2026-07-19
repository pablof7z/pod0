use pod0_domain::{NoteRecord, StateRevision, UnixTimestampMilliseconds};
use rusqlite::{Connection, OptionalExtension};

use crate::note_store_codec::{
    decode_author, decode_evidence, decode_kind, decode_target, note_id, note_revision,
};
use crate::{NoteCollectionSnapshot, StorageError};

pub(crate) fn require_notes_authoritative(connection: &Connection) -> Result<(), StorageError> {
    let state: Option<String> = connection
        .query_row(
            "SELECT state FROM pod0_domain_cutovers WHERE domain='notes'",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read notes authority", error))?;
    if state.as_deref() == Some("authoritative") {
        Ok(())
    } else {
        Err(StorageError::CutoverNotAuthoritative)
    }
}

pub(crate) fn read_note_snapshot(
    connection: &Connection,
) -> Result<NoteCollectionSnapshot, StorageError> {
    let revision: i64 = connection
        .query_row(
            "SELECT collection_revision FROM pod0_note_state WHERE singleton=1",
            [],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("read note collection revision", error))?;
    let revision = u64::try_from(revision).map_err(|_| StorageError::CorruptSchema {
        detail: "note collection revision is malformed",
    })?;
    let mut statement = connection
        .prepare(
            "SELECT note_id,note_revision,text,kind_code,kind_wire_code,author_code,author_wire_code,\
             target_code,target_wire_code,target_note_id,episode_id,position_ms,created_at_ms,deleted,\
             evidence_generation_id,evidence_transcript_version_id,evidence_content_digest,\
             evidence_span_id FROM pod0_notes ORDER BY created_at_ms DESC,note_id ASC",
        )
        .map_err(|error| StorageError::sqlite("prepare note projection", error))?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, Vec<u8>>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, Option<i64>>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, Option<i64>>(6)?,
                row.get::<_, i64>(7)?,
                row.get::<_, Option<i64>>(8)?,
                row.get::<_, Option<Vec<u8>>>(9)?,
                row.get::<_, Option<Vec<u8>>>(10)?,
                row.get::<_, Option<i64>>(11)?,
                row.get::<_, i64>(12)?,
                row.get::<_, i64>(13)?,
                row.get::<_, Option<Vec<u8>>>(14)?,
                row.get::<_, Option<Vec<u8>>>(15)?,
                row.get::<_, Option<Vec<u8>>>(16)?,
                row.get::<_, Option<Vec<u8>>>(17)?,
            ))
        })
        .map_err(|error| StorageError::sqlite("read note projection", error))?;
    let notes = rows
        .map(|row| {
            let row = row.map_err(|error| StorageError::sqlite("decode note row", error))?;
            Ok(NoteRecord {
                note_id: note_id(&row.0)?,
                revision: note_revision(row.1)?,
                text: row.2,
                kind: decode_kind(row.3, row.4)?,
                author: decode_author(row.5, row.6)?,
                target: decode_target(row.7, row.8, row.9, row.10, row.11)?,
                created_at: UnixTimestampMilliseconds::new(row.12),
                deleted: match row.13 {
                    0 => false,
                    1 => true,
                    _ => {
                        return Err(StorageError::CorruptSchema {
                            detail: "note deletion state is malformed",
                        });
                    }
                },
                evidence: decode_evidence(row.14, row.15, row.16, row.17)?,
            })
        })
        .collect::<Result<Vec<_>, StorageError>>()?;
    Ok(NoteCollectionSnapshot {
        revision: StateRevision::new(revision),
        notes,
    })
}
