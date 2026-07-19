use std::collections::BTreeSet;

use pod0_domain::{EpisodeId, QueueEntry, QueueEntryId};
use rusqlite::{OptionalExtension, Transaction, params};

use crate::StorageError;
use crate::library_store::source_import_id;
use crate::library_store_playback::PlaybackQueuePlacement;
use crate::library_store_playback_support::{
    active_episode, id_bytes, require_episode, segment_values,
};

type QueueHeadRow = (Vec<u8>, Vec<u8>, Option<i64>, Option<i64>, Option<String>);

pub(super) fn enqueue(
    transaction: &Transaction<'_>,
    entry: &QueueEntry,
    placement: PlaybackQueuePlacement,
) -> Result<(), StorageError> {
    require_episode(transaction, entry.episode_id)?;
    if entry.segment.is_none() {
        let active = active_episode(transaction)?;
        let duplicate: Option<i64> = transaction
            .query_row(
                "SELECT 1 FROM pod0_queue_entries WHERE episode_id=?1 AND \
             segment_start_ms IS NULL AND segment_end_ms IS NULL",
                [entry.episode_id.into_bytes().as_slice()],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| StorageError::sqlite("find duplicate queue entry", error))?;
        if active == Some(entry.episode_id) || duplicate.is_some() {
            return Ok(());
        }
    }
    let order = match placement {
        PlaybackQueuePlacement::Back => next_queue_order(transaction)?,
        PlaybackQueuePlacement::Next => {
            shift_queue(transaction)?;
            0
        }
    };
    let (start, end) = segment_values(entry.segment)?;
    let import_id = source_import_id(transaction)?;
    transaction
        .execute(
            "INSERT INTO pod0_queue_entries(queue_entry_id,sort_order,episode_id,segment_start_ms,\
         segment_end_ms,label,source_import_id) VALUES(?1,?2,?3,?4,?5,?6,?7)",
            params![
                entry.queue_entry_id.into_bytes().as_slice(),
                order,
                entry.episode_id.into_bytes().as_slice(),
                start,
                end,
                entry.label,
                import_id
            ],
        )
        .map_err(|error| StorageError::sqlite("enqueue playback item", error))?;
    Ok(())
}

pub(super) fn advance_queue(
    transaction: &Transaction<'_>,
) -> Result<Option<EpisodeId>, StorageError> {
    let head: Option<QueueHeadRow> = transaction
        .query_row(
            "SELECT queue_entry_id,episode_id,segment_start_ms,segment_end_ms,label \
             FROM pod0_queue_entries ORDER BY sort_order LIMIT 1",
            [],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read next queue entry", error))?;
    let Some((entry_id, episode_id, start, end, label)) = head else {
        transaction
            .execute(
                "UPDATE pod0_playback_state SET active_segment_start_ms=NULL,\
                 active_segment_end_ms=NULL,active_segment_label=NULL WHERE singleton=1",
                [],
            )
            .map_err(|error| StorageError::sqlite("clear exhausted playback segment", error))?;
        return Ok(None);
    };
    transaction
        .execute(
            "DELETE FROM pod0_queue_entries WHERE queue_entry_id=?1",
            [&entry_id],
        )
        .map_err(|error| StorageError::sqlite("dequeue playback item", error))?;
    normalize_queue(transaction)?;
    transaction
        .execute(
            "UPDATE pod0_playback_state SET active_episode_id=?1,active_segment_start_ms=?2,\
         active_segment_end_ms=?3,active_segment_label=?4 WHERE singleton=1",
            params![&episode_id, start, end, label],
        )
        .map_err(|error| StorageError::sqlite("advance active playback", error))?;
    let episode_id = EpisodeId::from_bytes(id_bytes(episode_id)?);
    transaction
        .execute(
            "UPDATE pod0_episodes SET completion_code=1,completion_cause_code=NULL,\
         completion_cause_wire_code=NULL WHERE episode_id=?1",
            [episode_id.into_bytes().as_slice()],
        )
        .map_err(|error| StorageError::sqlite("resume advanced episode", error))?;
    Ok(Some(episode_id))
}

pub(super) fn replace_queue_order(
    transaction: &Transaction<'_>,
    requested: &[QueueEntryId],
) -> Result<(), StorageError> {
    let current = queue_ids(transaction)?;
    let requested_set: BTreeSet<_> = requested.iter().copied().collect();
    if requested.len() != current.len() || requested_set != current.iter().copied().collect() {
        return Err(StorageError::EntityNotFound);
    }
    transaction
        .execute(
            "UPDATE pod0_queue_entries SET sort_order=sort_order+1000000",
            [],
        )
        .map_err(|error| StorageError::sqlite("stage queue reorder", error))?;
    for (index, id) in requested.iter().enumerate() {
        transaction
            .execute(
                "UPDATE pod0_queue_entries SET sort_order=?1 WHERE queue_entry_id=?2",
                params![
                    i64::try_from(index).map_err(|_| StorageError::CorruptSchema {
                        detail: "queue order exhausted",
                    })?,
                    id.into_bytes().as_slice()
                ],
            )
            .map_err(|error| StorageError::sqlite("reorder playback queue", error))?;
    }
    Ok(())
}

pub(super) fn normalize_queue(transaction: &Transaction<'_>) -> Result<(), StorageError> {
    let ids = queue_ids(transaction)?;
    transaction
        .execute(
            "UPDATE pod0_queue_entries SET sort_order=sort_order+1000000",
            [],
        )
        .map_err(|error| StorageError::sqlite("stage queue normalization", error))?;
    for (index, id) in ids.iter().enumerate() {
        transaction
            .execute(
                "UPDATE pod0_queue_entries SET sort_order=?1 WHERE queue_entry_id=?2",
                params![index as i64, id.into_bytes().as_slice()],
            )
            .map_err(|error| StorageError::sqlite("normalize playback queue", error))?;
    }
    Ok(())
}

fn queue_ids(transaction: &Transaction<'_>) -> Result<Vec<QueueEntryId>, StorageError> {
    let mut statement = transaction
        .prepare("SELECT queue_entry_id FROM pod0_queue_entries ORDER BY sort_order")
        .map_err(|error| StorageError::sqlite("prepare queue identities", error))?;
    let rows = statement
        .query_map([], |row| row.get::<_, Vec<u8>>(0))
        .map_err(|error| StorageError::sqlite("read queue identities", error))?;
    rows.map(|value| {
        value
            .map_err(|error| StorageError::sqlite("read queue identity", error))
            .and_then(id_bytes)
            .map(QueueEntryId::from_bytes)
    })
    .collect()
}

fn shift_queue(transaction: &Transaction<'_>) -> Result<(), StorageError> {
    transaction
        .execute(
            "UPDATE pod0_queue_entries SET sort_order=sort_order+1000000",
            [],
        )
        .map_err(|error| StorageError::sqlite("stage queue insertion", error))?;
    transaction
        .execute(
            "UPDATE pod0_queue_entries SET sort_order=sort_order-999999",
            [],
        )
        .map_err(|error| StorageError::sqlite("shift queue insertion", error))?;
    Ok(())
}

fn next_queue_order(transaction: &Transaction<'_>) -> Result<i64, StorageError> {
    transaction
        .query_row(
            "SELECT COALESCE(MAX(sort_order)+1,0) FROM pod0_queue_entries",
            [],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("read next queue order", error))
}
