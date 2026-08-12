use pod0_domain::{EpisodeId, PlaybackSegment};
use rusqlite::{OptionalExtension, Transaction};

use crate::StorageError;
use crate::listening_db_codec::i64_value;

pub(super) fn active_episode(
    transaction: &Transaction<'_>,
) -> Result<Option<EpisodeId>, StorageError> {
    let value: Option<Vec<u8>> = transaction
        .query_row(
            "SELECT active_episode_id FROM pod0_playback_state WHERE singleton=1",
            [],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("read active playback episode", error))?;
    value
        .map(id_bytes)
        .transpose()
        .map(|value| value.map(EpisodeId::from_bytes))
}

pub(super) fn require_episode(
    transaction: &Transaction<'_>,
    episode_id: EpisodeId,
) -> Result<(), StorageError> {
    let exists: Option<i64> = transaction
        .query_row(
            "SELECT 1 FROM pod0_episodes WHERE episode_id=?1",
            [episode_id.into_bytes().as_slice()],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| StorageError::sqlite("find playback episode", error))?;
    exists.map_or(Err(StorageError::EntityNotFound), |_| Ok(()))
}

pub(super) fn segment_values(
    segment: Option<PlaybackSegment>,
) -> Result<(Option<i64>, Option<i64>), StorageError> {
    let start = segment.and_then(|value| value.start_position_milliseconds);
    let end = segment.and_then(|value| value.end_position_milliseconds);
    if end.is_some_and(|value| value <= start.unwrap_or(0)) {
        return Err(StorageError::InvalidLegacyRecord {
            entity: "queue",
            index: 0,
            detail: "segment end must be greater than its start",
        });
    }
    Ok((
        start
            .map(|value| i64_value(value, "segment start"))
            .transpose()?,
        end.map(|value| i64_value(value, "segment end"))
            .transpose()?,
    ))
}

pub(super) fn id_bytes(value: Vec<u8>) -> Result<[u8; 16], StorageError> {
    value.try_into().map_err(|_| StorageError::CorruptSchema {
        detail: "playback identity must contain sixteen bytes",
    })
}
