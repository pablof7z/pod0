use pod0_domain::{EpisodeId, TranscriptArtifact, TranscriptArtifactId, TranscriptSegmentId};
use rusqlite::params;

use crate::StorageError;
use crate::transcript_store::TranscriptStore;
use crate::transcript_store_codec::{speaker_id, stored_u32, stored_u64};
use crate::transcript_store_model::{
    StoredTranscriptSegment, StoredTranscriptSpeaker, StoredTranscriptWord, TranscriptPage,
    TranscriptSelectionSummary,
};
use crate::transcript_store_read_artifact::read_artifact_by_id;
use crate::transcript_store_read_rows::{
    finish_page, page_limit, read_segments, read_summary, selected_artifact_id,
};

impl TranscriptStore {
    pub fn artifact(
        &self,
        artifact_id: TranscriptArtifactId,
    ) -> Result<Option<TranscriptArtifact>, StorageError> {
        self.read(|connection| read_artifact_by_id(connection, artifact_id))
    }

    pub fn selected_artifact(
        &self,
        episode_id: EpisodeId,
    ) -> Result<Option<TranscriptArtifact>, StorageError> {
        self.read(|connection| {
            let Some(artifact_id) = selected_artifact_id(connection, episode_id)? else {
                return Ok(None);
            };
            read_artifact_by_id(connection, artifact_id)
        })
    }

    pub fn selected_summary(
        &self,
        episode_id: EpisodeId,
    ) -> Result<Option<TranscriptSelectionSummary>, StorageError> {
        self.read(|connection| read_summary(connection, episode_id))
    }

    pub fn selected_speakers(
        &self,
        episode_id: EpisodeId,
        offset: u32,
        max_items: u16,
    ) -> Result<TranscriptPage<StoredTranscriptSpeaker>, StorageError> {
        self.read(|connection| {
            let Some(artifact_id) = selected_artifact_id(connection, episode_id)? else {
                return Err(StorageError::TranscriptNotFound);
            };
            let mut statement = connection
                .prepare(
                    "SELECT ordinal,speaker_id,label,display_name FROM pod0_transcript_speakers \
                     WHERE artifact_id=?1 ORDER BY ordinal LIMIT ?2 OFFSET ?3",
                )
                .map_err(|error| {
                    StorageError::sqlite("prepare bounded transcript speakers", error)
                })?;
            let (limit, fetch) = page_limit(max_items);
            let rows = statement
                .query_map(
                    params![
                        artifact_id.into_bytes().as_slice(),
                        fetch,
                        i64::from(offset)
                    ],
                    |row| {
                        Ok((
                            row.get::<_, i64>(0)?,
                            row.get::<_, Vec<u8>>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, Option<String>>(3)?,
                        ))
                    },
                )
                .map_err(|error| StorageError::sqlite("read bounded transcript speakers", error))?;
            let items = rows
                .map(|row| {
                    let row = row.map_err(|error| {
                        StorageError::sqlite("decode transcript speaker", error)
                    })?;
                    Ok(StoredTranscriptSpeaker {
                        ordinal: stored_u32(row.0, "transcript speaker ordinal")?,
                        speaker_id: speaker_id(&row.1)?,
                        label: row.2,
                        display_name: row.3,
                    })
                })
                .collect::<Result<Vec<_>, StorageError>>()?;
            Ok(finish_page(items, limit))
        })
    }

    pub fn selected_segments(
        &self,
        episode_id: EpisodeId,
        offset: u32,
        max_items: u16,
    ) -> Result<TranscriptPage<StoredTranscriptSegment>, StorageError> {
        self.read(|connection| {
            let Some(artifact_id) = selected_artifact_id(connection, episode_id)? else {
                return Err(StorageError::TranscriptNotFound);
            };
            read_segments(connection, artifact_id, None, offset, max_items)
        })
    }

    pub fn selected_segment(
        &self,
        episode_id: EpisodeId,
        requested_segment: TranscriptSegmentId,
    ) -> Result<Option<StoredTranscriptSegment>, StorageError> {
        self.read(|connection| {
            let Some(artifact_id) = selected_artifact_id(connection, episode_id)? else {
                return Ok(None);
            };
            Ok(
                read_segments(connection, artifact_id, Some(requested_segment), 0, 1)?
                    .items
                    .into_iter()
                    .next(),
            )
        })
    }

    pub fn selected_words(
        &self,
        episode_id: EpisodeId,
        requested_segment: TranscriptSegmentId,
        offset: u32,
        max_items: u16,
    ) -> Result<TranscriptPage<StoredTranscriptWord>, StorageError> {
        self.read(|connection| {
            let Some(artifact_id) = selected_artifact_id(connection, episode_id)? else {
                return Err(StorageError::TranscriptNotFound);
            };
            let mut statement = connection
                .prepare(
                    "SELECT w.ordinal,w.text,w.start_ms,w.end_ms FROM pod0_transcript_words w \
                     JOIN pod0_transcript_artifact_segments s ON s.artifact_id=w.artifact_id \
                     AND s.segment_id=w.segment_id WHERE w.artifact_id=?1 AND w.segment_id=?2 \
                     ORDER BY w.ordinal LIMIT ?3 OFFSET ?4",
                )
                .map_err(|error| StorageError::sqlite("prepare bounded transcript words", error))?;
            let (limit, fetch) = page_limit(max_items);
            let rows = statement
                .query_map(
                    params![
                        artifact_id.into_bytes().as_slice(),
                        requested_segment.into_bytes().as_slice(),
                        fetch,
                        i64::from(offset),
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
                .map_err(|error| StorageError::sqlite("read bounded transcript words", error))?;
            let items = rows
                .map(|row| {
                    let row =
                        row.map_err(|error| StorageError::sqlite("decode transcript word", error))?;
                    Ok(StoredTranscriptWord {
                        segment_id: requested_segment,
                        ordinal: stored_u32(row.0, "transcript word ordinal")?,
                        text: row.1,
                        start_milliseconds: stored_u64(row.2, "transcript word start")?,
                        end_milliseconds: stored_u64(row.3, "transcript word end")?,
                    })
                })
                .collect::<Result<Vec<_>, StorageError>>()?;
            Ok(finish_page(items, limit))
        })
    }
}
