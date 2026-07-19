use pod0_domain::{
    TranscriptArtifact, TranscriptArtifactId, TranscriptArtifactInput,
    TranscriptArtifactSegmentInput, TranscriptArtifactSpeakerInput, TranscriptArtifactWordInput,
    UnixTimestampMilliseconds,
};
use rusqlite::{Connection, OptionalExtension};

use crate::StorageError;
use crate::listening_db_codec::decode_transcript_source;
use crate::transcript_store_codec::{
    artifact_error, artifact_id, digest, episode_id, optional_speaker_id, podcast_id, segment_id,
    speaker_id, stored_u32, stored_u64, version_id,
};

type HeaderRow = (
    Vec<u8>,
    Vec<u8>,
    Vec<u8>,
    Vec<u8>,
    i64,
    Vec<u8>,
    String,
    Vec<u8>,
    i64,
    Option<i64>,
    Option<String>,
    Vec<u8>,
    String,
    i64,
    i64,
    i64,
    i64,
);

pub(crate) fn read_artifact_by_id(
    connection: &Connection,
    requested_id: TranscriptArtifactId,
) -> Result<Option<TranscriptArtifact>, StorageError> {
    let row = connection
        .query_row(
            "SELECT a.artifact_id,a.transcript_version_id,a.episode_id,d.podcast_id,\
             a.schema_version,a.integrity_digest,d.source_revision,d.content_digest,d.source_code,\
             d.source_wire_code,d.provider,d.source_payload_digest,a.language,a.generated_at_ms,\
             a.speaker_count,a.segment_count,a.word_count \
             FROM pod0_transcript_artifacts a JOIN pod0_transcript_documents d \
             ON d.transcript_version_id=a.transcript_version_id WHERE a.artifact_id=?1",
            [requested_id.into_bytes().as_slice()],
            |row| {
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
                    row.get(12)?,
                    row.get(13)?,
                    row.get(14)?,
                    row.get(15)?,
                    row.get(16)?,
                ))
            },
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read transcript artifact header", error))?;
    row.map(|row| decode_artifact(connection, row)).transpose()
}

fn decode_artifact(
    connection: &Connection,
    row: HeaderRow,
) -> Result<TranscriptArtifact, StorageError> {
    let stored_artifact_id = artifact_id(&row.0)?;
    let stored_version_id = version_id(&row.1)?;
    let episode_id = episode_id(&row.2)?;
    let podcast_id = podcast_id(&row.3)?;
    let schema_version = stored_u32(row.4, "transcript artifact schema")?;
    let integrity_digest = digest(&row.5)?;
    let content_digest = digest(&row.7)?;
    let source = decode_transcript_source(row.8, row.9)?;
    let source_payload_digest = digest(&row.11)?;
    let speaker_count = stored_u64(row.14, "transcript speaker count")?;
    let segment_count = stored_u64(row.15, "transcript segment count")?;
    let word_count = stored_u64(row.16, "transcript word count")?;
    let speakers = read_speakers(connection, stored_artifact_id)?;
    let segments = read_segments(connection, stored_artifact_id)?;
    let actual_words = segments
        .iter()
        .try_fold(0_u64, |total, segment| {
            total.checked_add(segment.words.len() as u64)
        })
        .ok_or(StorageError::InvalidTranscriptArtifact)?;
    if speaker_count != speakers.len() as u64
        || segment_count != segments.len() as u64
        || word_count != actual_words
    {
        return Err(StorageError::InvalidTranscriptArtifact);
    }
    let artifact = TranscriptArtifact::seal(TranscriptArtifactInput {
        episode_id,
        podcast_id,
        source_revision: row.6,
        source,
        provider: row.10,
        source_payload_digest,
        language: row.12,
        generated_at: UnixTimestampMilliseconds::new(row.13),
        speakers,
        segments,
    })
    .map_err(artifact_error)?;
    if artifact.schema_version != schema_version
        || artifact.artifact_id != stored_artifact_id
        || artifact.transcript_version_id != stored_version_id
        || artifact.content_digest != content_digest
        || artifact.integrity_digest != integrity_digest
    {
        return Err(StorageError::InvalidTranscriptArtifact);
    }
    Ok(artifact)
}

fn read_speakers(
    connection: &Connection,
    artifact_id_value: TranscriptArtifactId,
) -> Result<Vec<TranscriptArtifactSpeakerInput>, StorageError> {
    let mut statement = connection
        .prepare(
            "SELECT ordinal,speaker_id,label,display_name FROM pod0_transcript_speakers \
             WHERE artifact_id=?1 ORDER BY ordinal",
        )
        .map_err(|error| StorageError::sqlite("prepare transcript speakers", error))?;
    let rows = statement
        .query_map([artifact_id_value.into_bytes().as_slice()], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, Vec<u8>>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
            ))
        })
        .map_err(|error| StorageError::sqlite("read transcript speakers", error))?;
    rows.enumerate()
        .map(|(expected, row)| {
            let row =
                row.map_err(|error| StorageError::sqlite("decode transcript speaker", error))?;
            if usize::try_from(row.0).ok() != Some(expected) {
                return Err(StorageError::InvalidTranscriptArtifact);
            }
            Ok(TranscriptArtifactSpeakerInput {
                speaker_id: speaker_id(&row.1)?,
                label: row.2,
                display_name: row.3,
            })
        })
        .collect()
}

fn read_segments(
    connection: &Connection,
    artifact_id_value: TranscriptArtifactId,
) -> Result<Vec<TranscriptArtifactSegmentInput>, StorageError> {
    let mut statement = connection
        .prepare(
            "SELECT a.ordinal,a.segment_id,a.raw_text,s.start_ms,s.end_ms,s.speaker_id,a.word_count \
             FROM pod0_transcript_artifact_segments a JOIN pod0_transcript_segments s \
             ON s.segment_id=a.segment_id AND s.transcript_version_id=a.transcript_version_id \
             WHERE a.artifact_id=?1 ORDER BY a.ordinal",
        )
        .map_err(|error| StorageError::sqlite("prepare transcript artifact segments", error))?;
    let rows = statement
        .query_map([artifact_id_value.into_bytes().as_slice()], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, Vec<u8>>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, Option<Vec<u8>>>(5)?,
                row.get::<_, i64>(6)?,
            ))
        })
        .map_err(|error| StorageError::sqlite("read transcript artifact segments", error))?;
    rows.enumerate()
        .map(|(expected, row)| {
            let row =
                row.map_err(|error| StorageError::sqlite("decode transcript segment", error))?;
            if usize::try_from(row.0).ok() != Some(expected) {
                return Err(StorageError::InvalidTranscriptArtifact);
            }
            let segment_id = segment_id(&row.1)?;
            let words = read_words(connection, artifact_id_value, segment_id)?;
            if u64::try_from(row.6).ok() != Some(words.len() as u64) {
                return Err(StorageError::InvalidTranscriptArtifact);
            }
            Ok(TranscriptArtifactSegmentInput {
                text: row.2,
                start_milliseconds: stored_u64(row.3, "transcript segment start")?,
                end_milliseconds: stored_u64(row.4, "transcript segment end")?,
                speaker_id: optional_speaker_id(row.5)?,
                words,
            })
        })
        .collect()
}

fn read_words(
    connection: &Connection,
    artifact_id_value: TranscriptArtifactId,
    segment_id_value: pod0_domain::TranscriptSegmentId,
) -> Result<Vec<TranscriptArtifactWordInput>, StorageError> {
    let mut statement = connection
        .prepare(
            "SELECT ordinal,text,start_ms,end_ms FROM pod0_transcript_words \
             WHERE artifact_id=?1 AND segment_id=?2 ORDER BY ordinal",
        )
        .map_err(|error| StorageError::sqlite("prepare transcript artifact words", error))?;
    let rows = statement
        .query_map(
            [
                artifact_id_value.into_bytes().as_slice(),
                segment_id_value.into_bytes().as_slice(),
            ],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            },
        )
        .map_err(|error| StorageError::sqlite("read transcript artifact words", error))?;
    rows.enumerate()
        .map(|(expected, row)| {
            let row = row.map_err(|error| StorageError::sqlite("decode transcript word", error))?;
            if usize::try_from(row.0).ok() != Some(expected) {
                return Err(StorageError::InvalidTranscriptArtifact);
            }
            Ok(TranscriptArtifactWordInput {
                text: row.1,
                start_milliseconds: stored_u64(row.2, "transcript word start")?,
                end_milliseconds: stored_u64(row.3, "transcript word end")?,
            })
        })
        .collect()
}
