use pod0_domain::{CommandId, TranscriptArtifact};
use rusqlite::{OptionalExtension, Transaction, params};

use crate::StorageError;
use crate::listening_db_codec::{decode_transcript_source, transcript_source};
use crate::transcript_store_codec::{artifact_error, optional_speaker_id, segment_id, sqlite_i64};
use crate::transcript_store_read_artifact::read_artifact_by_id;
use crate::transcript_store_write_artifact::insert_artifact_rows;

pub(crate) fn require_episode_parent(
    transaction: &Transaction<'_>,
    artifact: &TranscriptArtifact,
) -> Result<(), StorageError> {
    let parent: Option<Vec<u8>> = transaction
        .query_row(
            "SELECT podcast_id FROM pod0_episodes WHERE episode_id=?1",
            [artifact.episode_id.into_bytes().as_slice()],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read transcript episode parent", error))?;
    match parent {
        Some(parent) if parent == artifact.podcast_id.into_bytes() => Ok(()),
        Some(_) => Err(StorageError::InvalidTranscriptArtifact),
        None => Err(StorageError::TranscriptNotFound),
    }
}

pub(crate) fn ensure_semantic_document(
    transaction: &Transaction<'_>,
    artifact: &TranscriptArtifact,
) -> Result<(), StorageError> {
    let version = artifact.transcript_version_id.into_bytes();
    let stored = transaction
        .query_row(
            "SELECT episode_id,podcast_id,source_revision,content_digest,source_code,\
             source_wire_code,provider,source_payload_digest,segment_count \
             FROM pod0_transcript_documents WHERE transcript_version_id=?1",
            [version.as_slice()],
            |row| {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, Vec<u8>>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Vec<u8>>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, Option<i64>>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, Vec<u8>>(7)?,
                    row.get::<_, i64>(8)?,
                ))
            },
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read semantic transcript document", error))?;
    if let Some(stored) = stored {
        let source = decode_transcript_source(stored.4, stored.5)?;
        let segment_count =
            usize::try_from(stored.8).map_err(|_| StorageError::InvalidTranscriptArtifact)?;
        if stored.0 != artifact.episode_id.into_bytes()
            || stored.1 != artifact.podcast_id.into_bytes()
            || stored.2 != artifact.source_revision
            || stored.3 != artifact.content_digest.into_bytes()
            || source != artifact.provenance.source
            || stored.6 != artifact.provenance.provider
            || stored.7 != artifact.provenance.source_payload_digest.into_bytes()
            || segment_count != artifact.segments.len()
        {
            return Err(StorageError::InvalidTranscriptArtifact);
        }
        return verify_semantic_segments(transaction, artifact);
    }

    let (source_code, source_wire) = transcript_source(&artifact.provenance.source);
    let segment_count = count(artifact.segments.len())?;
    transaction
        .execute(
            "INSERT INTO pod0_transcript_documents(transcript_version_id,episode_id,podcast_id,\
             source_revision,content_digest,source_code,source_wire_code,provider,\
             source_payload_digest,segment_count) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
            params![
                version.as_slice(),
                artifact.episode_id.into_bytes().as_slice(),
                artifact.podcast_id.into_bytes().as_slice(),
                artifact.source_revision,
                artifact.content_digest.into_bytes().as_slice(),
                source_code,
                source_wire,
                artifact.provenance.provider,
                artifact
                    .provenance
                    .source_payload_digest
                    .into_bytes()
                    .as_slice(),
                segment_count,
            ],
        )
        .map_err(|error| StorageError::sqlite("insert semantic transcript document", error))?;
    let mut statement = transaction
        .prepare(
            "INSERT INTO pod0_transcript_segments(segment_id,transcript_version_id,ordinal,text,\
             start_ms,end_ms,speaker_id) VALUES(?1,?2,?3,?4,?5,?6,?7)",
        )
        .map_err(|error| StorageError::sqlite("prepare semantic transcript segments", error))?;
    for segment in &artifact.segments {
        statement
            .execute(params![
                segment.segment_id.into_bytes().as_slice(),
                version.as_slice(),
                segment.ordinal,
                normalize(&segment.text),
                sqlite_i64(segment.start_milliseconds)?,
                sqlite_i64(segment.end_milliseconds)?,
                segment.speaker_id.map(|id| id.into_bytes().to_vec()),
            ])
            .map_err(|error| StorageError::sqlite("insert semantic transcript segment", error))?;
    }
    Ok(())
}

pub(crate) fn insert_or_validate_artifact(
    transaction: &Transaction<'_>,
    artifact: &TranscriptArtifact,
    source_import_id: Option<CommandId>,
    created_at_ms: i64,
) -> Result<bool, StorageError> {
    artifact.verify_integrity().map_err(artifact_error)?;
    if created_at_ms < 0 || artifact.generated_at.value < 0 {
        return Err(StorageError::InvalidTranscriptArtifact);
    }
    let artifact_id = artifact.artifact_id.into_bytes();
    let exists: bool = transaction
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM pod0_transcript_artifacts WHERE artifact_id=?1)",
            [artifact_id.as_slice()],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("check transcript artifact", error))?;
    if exists {
        let stored = read_artifact_by_id(transaction, artifact.artifact_id)?
            .ok_or(StorageError::InvalidTranscriptArtifact)?;
        return if stored == *artifact {
            Ok(true)
        } else {
            Err(StorageError::InvalidTranscriptArtifact)
        };
    }
    insert_artifact_rows(transaction, artifact, source_import_id, created_at_ms)?;
    Ok(false)
}

fn verify_semantic_segments(
    transaction: &Transaction<'_>,
    artifact: &TranscriptArtifact,
) -> Result<(), StorageError> {
    let mut statement = transaction
        .prepare(
            "SELECT segment_id,ordinal,text,start_ms,end_ms,speaker_id \
             FROM pod0_transcript_segments WHERE transcript_version_id=?1 ORDER BY ordinal",
        )
        .map_err(|error| StorageError::sqlite("prepare semantic transcript verification", error))?;
    let rows = statement
        .query_map(
            [artifact.transcript_version_id.into_bytes().as_slice()],
            |row| {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, Option<Vec<u8>>>(5)?,
                ))
            },
        )
        .map_err(|error| StorageError::sqlite("read semantic transcript segments", error))?;
    let rows = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| StorageError::sqlite("decode semantic transcript segments", error))?;
    if rows.len() != artifact.segments.len() {
        return Err(StorageError::InvalidTranscriptArtifact);
    }
    for (stored, expected) in rows.into_iter().zip(&artifact.segments) {
        if segment_id(&stored.0)? != expected.segment_id
            || u32::try_from(stored.1).ok() != Some(expected.ordinal)
            || stored.2 != normalize(&expected.text)
            || u64::try_from(stored.3).ok() != Some(expected.start_milliseconds)
            || u64::try_from(stored.4).ok() != Some(expected.end_milliseconds)
            || optional_speaker_id(stored.5)? != expected.speaker_id
        {
            return Err(StorageError::InvalidTranscriptArtifact);
        }
    }
    Ok(())
}

fn normalize(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn count(value: usize) -> Result<i64, StorageError> {
    i64::try_from(value).map_err(|_| StorageError::InvalidTranscriptArtifact)
}
