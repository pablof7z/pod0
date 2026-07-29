use pod0_domain::{SpeakerEntityId, TranscriptArtifactId};
use rusqlite::OptionalExtension;

use crate::StorageError;
use crate::speaker_store_model::{
    StoredSpeakerAssignment, StoredSpeakerEntity, decode_assignment_origin,
};
use crate::transcript_store::TranscriptStore;
use crate::transcript_store_codec::{speaker_entity_id, speaker_id, stored_u64};

impl TranscriptStore {
    pub fn speaker_entity(
        &self,
        requested: SpeakerEntityId,
    ) -> Result<Option<StoredSpeakerEntity>, StorageError> {
        self.read(|connection| {
            connection
                .query_row(
                    "SELECT speaker_entity_revision,display_name,deleted FROM pod0_speakers \
                     WHERE speaker_entity_id=?1",
                    [requested.into_bytes().as_slice()],
                    |row| {
                        Ok((
                            row.get::<_, i64>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, i64>(2)?,
                        ))
                    },
                )
                .optional()
                .map_err(|error| StorageError::sqlite("read speaker entity", error))?
                .map(|row| {
                    Ok(StoredSpeakerEntity {
                        speaker_entity_id: requested,
                        revision: stored_u64(row.0, "speaker entity revision")?,
                        display_name: row.1,
                        deleted: row.2 != 0,
                    })
                })
                .transpose()
        })
    }

    /// All assignments for one artifact, ordered by speaker id, bounded by
    /// the artifact speaker cap (4096) rather than a page cursor.
    pub fn speaker_assignments(
        &self,
        artifact_id: TranscriptArtifactId,
    ) -> Result<Vec<StoredSpeakerAssignment>, StorageError> {
        self.read(|connection| {
            let mut statement = connection
                .prepare(
                    "SELECT speaker_id,speaker_entity_id,confidence,origin_code,decided_at_ms \
                     FROM pod0_speaker_assignments WHERE artifact_id=?1 ORDER BY speaker_id",
                )
                .map_err(|error| StorageError::sqlite("prepare speaker assignments", error))?;
            let rows = statement
                .query_map([artifact_id.into_bytes().as_slice()], |row| {
                    Ok((
                        row.get::<_, Vec<u8>>(0)?,
                        row.get::<_, Vec<u8>>(1)?,
                        row.get::<_, Option<f64>>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, i64>(4)?,
                    ))
                })
                .map_err(|error| StorageError::sqlite("read speaker assignments", error))?;
            rows.map(|row| {
                let row =
                    row.map_err(|error| StorageError::sqlite("decode speaker assignment", error))?;
                Ok(StoredSpeakerAssignment {
                    artifact_id,
                    speaker_id: speaker_id(&row.0)?,
                    speaker_entity_id: speaker_entity_id(&row.1)?,
                    confidence: row.2,
                    origin: decode_assignment_origin(row.3)?,
                    decided_at_ms: row.4,
                })
            })
            .collect()
        })
    }
}
