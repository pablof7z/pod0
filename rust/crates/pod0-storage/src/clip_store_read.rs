use pod0_domain::{ClipRecord, StateRevision, UnixTimestampMilliseconds};
use rusqlite::{Connection, OptionalExtension};

use crate::clip_store_codec::{
    clip_id, clip_revision, decode_evidence, decode_source, episode_id, podcast_id, speaker_id,
};
use crate::{ClipCollectionSnapshot, StorageError};

pub(crate) fn require_clips_authoritative(connection: &Connection) -> Result<(), StorageError> {
    let state: Option<String> = connection
        .query_row(
            "SELECT state FROM pod0_domain_cutovers WHERE domain='clips'",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read clips authority", error))?;
    if state.as_deref() == Some("authoritative") {
        Ok(())
    } else {
        Err(StorageError::CutoverNotAuthoritative)
    }
}

pub(crate) fn read_clip_snapshot(
    connection: &Connection,
) -> Result<ClipCollectionSnapshot, StorageError> {
    let revision: i64 = connection
        .query_row(
            "SELECT collection_revision FROM pod0_clip_state WHERE singleton=1",
            [],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("read clip collection revision", error))?;
    let revision = u64::try_from(revision).map_err(|_| StorageError::CorruptSchema {
        detail: "clip collection revision is malformed",
    })?;
    let mut statement = connection
        .prepare(
            "SELECT clip_id,clip_revision,episode_id,podcast_id,start_ms,end_ms,created_at_ms,\
             caption,speaker_id,speaker_label,frozen_transcript_text,source_code,source_wire_code,deleted,\
             evidence_generation_id,evidence_transcript_version_id,evidence_content_digest,\
             evidence_span_id FROM pod0_clips ORDER BY created_at_ms DESC,clip_id ASC",
        )
        .map_err(|error| StorageError::sqlite("prepare clip projection", error))?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, Vec<u8>>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, Vec<u8>>(2)?,
                row.get::<_, Vec<u8>>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, i64>(6)?,
                row.get::<_, Option<String>>(7)?,
                row.get::<_, Option<Vec<u8>>>(8)?,
                row.get::<_, Option<String>>(9)?,
                row.get::<_, String>(10)?,
                row.get::<_, i64>(11)?,
                row.get::<_, Option<i64>>(12)?,
                row.get::<_, i64>(13)?,
                row.get::<_, Option<Vec<u8>>>(14)?,
                row.get::<_, Option<Vec<u8>>>(15)?,
                row.get::<_, Option<Vec<u8>>>(16)?,
                row.get::<_, Option<Vec<u8>>>(17)?,
            ))
        })
        .map_err(|error| StorageError::sqlite("read clip projection", error))?;
    let clips = rows
        .map(|row| {
            let row = row.map_err(|error| StorageError::sqlite("decode clip row", error))?;
            Ok(ClipRecord {
                clip_id: clip_id(&row.0)?,
                revision: clip_revision(row.1)?,
                episode_id: episode_id(&row.2)?,
                podcast_id: podcast_id(&row.3)?,
                start_milliseconds: u64::try_from(row.4).map_err(|_| corrupt("clip start"))?,
                end_milliseconds: u64::try_from(row.5).map_err(|_| corrupt("clip end"))?,
                created_at: UnixTimestampMilliseconds::new(row.6),
                caption: row.7,
                speaker_id: speaker_id(row.8)?,
                speaker_label: row.9,
                frozen_transcript_text: row.10,
                source: decode_source(row.11, row.12)?,
                deleted: match row.13 {
                    0 => false,
                    1 => true,
                    _ => return Err(corrupt("clip deletion state")),
                },
                evidence: decode_evidence(row.14, row.15, row.16, row.17)?,
            })
        })
        .collect::<Result<Vec<_>, StorageError>>()?;
    Ok(ClipCollectionSnapshot {
        revision: StateRevision::new(revision),
        clips,
    })
}

fn corrupt(detail: &'static str) -> StorageError {
    StorageError::CorruptSchema { detail }
}
