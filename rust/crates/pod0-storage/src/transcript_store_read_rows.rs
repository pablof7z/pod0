use pod0_domain::{
    EpisodeId, TranscriptArtifactId, TranscriptSegmentId, UnixTimestampMilliseconds,
};
use rusqlite::{Connection, OptionalExtension, params};

use crate::StorageError;
use crate::listening_db_codec::decode_transcript_source;
use crate::transcript_store_codec::{
    artifact_id, digest, optional_speaker_id, podcast_id, revision, segment_id, stored_u32,
    stored_u64, version_id,
};
use crate::transcript_store_model::{
    MAX_TRANSCRIPT_PROJECTION_ITEMS, StoredTranscriptSegment, TranscriptPage,
    TranscriptSelectionSummary,
};

pub(crate) fn selected_artifact_id(
    connection: &Connection,
    episode_id: EpisodeId,
) -> Result<Option<TranscriptArtifactId>, StorageError> {
    let value: Option<Vec<u8>> = connection
        .query_row(
            "SELECT artifact_id FROM pod0_transcript_selection WHERE episode_id=?1",
            [episode_id.into_bytes().as_slice()],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read selected transcript identity", error))?;
    value.as_deref().map(artifact_id).transpose()
}

#[allow(clippy::type_complexity)]
pub(crate) fn read_summary(
    connection: &Connection,
    episode_id_value: EpisodeId,
) -> Result<Option<TranscriptSelectionSummary>, StorageError> {
    let row = connection
        .query_row(
            "SELECT a.artifact_id,a.transcript_version_id,d.podcast_id,d.source_revision,\
             d.source_code,d.source_wire_code,d.provider,d.source_payload_digest,a.language,\
             a.generated_at_ms,d.content_digest,a.integrity_digest,s.selection_revision,\
             a.speaker_count,a.segment_count,a.word_count FROM pod0_transcript_selection s \
             JOIN pod0_transcript_artifacts a ON a.artifact_id=s.artifact_id \
             JOIN pod0_transcript_documents d ON d.transcript_version_id=a.transcript_version_id \
             WHERE s.episode_id=?1",
            [episode_id_value.into_bytes().as_slice()],
            |row| {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, Vec<u8>>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, Option<i64>>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, Vec<u8>>(7)?,
                    row.get::<_, String>(8)?,
                    row.get::<_, i64>(9)?,
                    row.get::<_, Vec<u8>>(10)?,
                    row.get::<_, Vec<u8>>(11)?,
                    row.get::<_, i64>(12)?,
                    row.get::<_, i64>(13)?,
                    row.get::<_, i64>(14)?,
                    row.get::<_, i64>(15)?,
                ))
            },
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read selected transcript summary", error))?;
    row.map(|row| {
        Ok(TranscriptSelectionSummary {
            artifact_id: artifact_id(&row.0)?,
            transcript_version_id: version_id(&row.1)?,
            episode_id: episode_id_value,
            podcast_id: podcast_id(&row.2)?,
            source_revision: row.3,
            source: decode_transcript_source(row.4, row.5)?,
            provider: row.6,
            source_payload_digest: digest(&row.7)?,
            language: row.8,
            generated_at: UnixTimestampMilliseconds::new(row.9),
            transcript_content_digest: digest(&row.10)?,
            artifact_integrity_digest: digest(&row.11)?,
            selection_revision: revision(row.12)?,
            speaker_count: stored_u32(row.13, "transcript speaker count")?,
            segment_count: stored_u32(row.14, "transcript segment count")?,
            word_count: stored_u64(row.15, "transcript word count")?,
        })
    })
    .transpose()
}

pub(crate) fn read_segments(
    connection: &Connection,
    artifact_id_value: TranscriptArtifactId,
    requested: Option<TranscriptSegmentId>,
    offset: u32,
    max_items: u16,
) -> Result<TranscriptPage<StoredTranscriptSegment>, StorageError> {
    let (limit, fetch) = page_limit(max_items);
    let mut statement = connection
        .prepare(
            "SELECT a.segment_id,a.ordinal,a.raw_text,s.start_ms,s.end_ms,s.speaker_id,a.word_count \
             FROM pod0_transcript_artifact_segments a JOIN pod0_transcript_segments s \
             ON s.segment_id=a.segment_id AND s.transcript_version_id=a.transcript_version_id \
             WHERE a.artifact_id=?1 AND (?2 IS NULL OR a.segment_id=?2) \
             ORDER BY a.ordinal LIMIT ?3 OFFSET ?4",
        )
        .map_err(|error| StorageError::sqlite("prepare bounded transcript segments", error))?;
    let rows = statement
        .query_map(
            params![
                artifact_id_value.into_bytes().as_slice(),
                requested.map(|id| id.into_bytes().to_vec()),
                fetch,
                i64::from(offset)
            ],
            |row| {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, Option<Vec<u8>>>(5)?,
                    row.get::<_, i64>(6)?,
                ))
            },
        )
        .map_err(|error| StorageError::sqlite("read bounded transcript segments", error))?;
    let items = rows
        .map(|row| {
            let row =
                row.map_err(|error| StorageError::sqlite("decode transcript segment", error))?;
            Ok(StoredTranscriptSegment {
                segment_id: segment_id(&row.0)?,
                ordinal: stored_u32(row.1, "transcript segment ordinal")?,
                text: row.2,
                start_milliseconds: stored_u64(row.3, "transcript segment start")?,
                end_milliseconds: stored_u64(row.4, "transcript segment end")?,
                speaker_id: optional_speaker_id(row.5)?,
                word_count: stored_u32(row.6, "transcript segment word count")?,
            })
        })
        .collect::<Result<Vec<_>, StorageError>>()?;
    Ok(finish_page(items, limit))
}

pub(crate) fn page_limit(requested: u16) -> (usize, i64) {
    let limit = usize::from(requested.clamp(1, MAX_TRANSCRIPT_PROJECTION_ITEMS));
    (
        limit,
        i64::try_from(limit + 1).expect("bounded transcript page limit"),
    )
}

pub(crate) fn finish_page<T>(mut items: Vec<T>, limit: usize) -> TranscriptPage<T> {
    let has_more = items.len() > limit;
    items.truncate(limit);
    TranscriptPage { items, has_more }
}
