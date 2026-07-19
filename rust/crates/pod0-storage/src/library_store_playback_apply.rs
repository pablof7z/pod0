use pod0_domain::{CompletionCause, CompletionStatus, EpisodeId};
use rusqlite::{Transaction, params};

use crate::StorageError;
use crate::library_store_playback::PlaybackMutation;
use crate::library_store_playback_queue::{
    advance_queue, enqueue, normalize_queue, replace_queue_order,
};
use crate::library_store_playback_support::{require_episode, segment_values};
use crate::listening_db_codec::{bool_value, completion, i64_value, sleep};

pub(super) fn apply_mutation(
    transaction: &Transaction<'_>,
    mutation: PlaybackMutation,
    observed_at_ms: i64,
) -> Result<(), StorageError> {
    match mutation {
        PlaybackMutation::Select {
            episode_id,
            segment,
            label,
        } => {
            require_episode(transaction, episode_id)?;
            let (start, end) = segment_values(segment)?;
            transaction.execute(
                "UPDATE pod0_playback_state SET active_episode_id=?1,active_segment_start_ms=?2,\
                 active_segment_end_ms=?3,active_segment_label=?4 WHERE singleton=1",
                params![episode_id.into_bytes().as_slice(), start, end, label],
            ).map_err(|error| StorageError::sqlite("select active playback", error))?;
            transaction
                .execute(
                    "UPDATE pod0_episodes SET completion_code=1,completion_cause_code=NULL,\
                 completion_cause_wire_code=NULL WHERE episode_id=?1",
                    [episode_id.into_bytes().as_slice()],
                )
                .map_err(|error| StorageError::sqlite("resume selected episode", error))?;
        }
        PlaybackMutation::Enqueue { entry, placement } => enqueue(transaction, &entry, placement)?,
        PlaybackMutation::RemoveQueueEntry(id) => {
            transaction
                .execute(
                    "DELETE FROM pod0_queue_entries WHERE queue_entry_id=?1",
                    [id.into_bytes().as_slice()],
                )
                .map_err(|error| StorageError::sqlite("remove queue entry", error))?;
            normalize_queue(transaction)?;
        }
        PlaybackMutation::RemoveEpisode(id) => {
            transaction
                .execute(
                    "DELETE FROM pod0_queue_entries WHERE episode_id=?1",
                    [id.into_bytes().as_slice()],
                )
                .map_err(|error| StorageError::sqlite("remove episode from queue", error))?;
            normalize_queue(transaction)?;
        }
        PlaybackMutation::ReplaceQueueOrder(ids) => replace_queue_order(transaction, &ids)?,
        PlaybackMutation::ClearQueue => {
            transaction
                .execute("DELETE FROM pod0_queue_entries", [])
                .map_err(|error| StorageError::sqlite("clear playback queue", error))?;
        }
        PlaybackMutation::AdvanceQueue => {
            advance_queue(transaction)?;
        }
        PlaybackMutation::SetRate(rate) => set_rate(transaction, rate.value)?,
        PlaybackMutation::SetSleepTimer(mode) => {
            let (code, duration, wire) = sleep(&mode)?;
            transaction
                .execute(
                    "UPDATE pod0_playback_state SET sleep_mode_code=?1,sleep_duration_ms=?2,\
                 sleep_wire_code=?3 WHERE singleton=1",
                    params![code, duration, wire],
                )
                .map_err(|error| StorageError::sqlite("set playback sleep timer", error))?;
        }
        PlaybackMutation::SetPreferences {
            auto_mark_played_at_natural_end,
            auto_play_next,
        } => {
            transaction
                .execute(
                    "UPDATE pod0_playback_state SET auto_mark_played_at_natural_end=?1,\
                 auto_play_next=?2 WHERE singleton=1",
                    params![
                        bool_value(auto_mark_played_at_natural_end),
                        bool_value(auto_play_next)
                    ],
                )
                .map_err(|error| StorageError::sqlite("set playback preferences", error))?;
        }
        PlaybackMutation::SetCompletion {
            episode_id,
            completion: value,
        } => set_completion(transaction, episode_id, value)?,
        PlaybackMutation::ResetProgress(episode_id) => reset_progress(transaction, episode_id)?,
        PlaybackMutation::Checkpoint {
            episode_id,
            position_milliseconds,
        } => checkpoint(
            transaction,
            episode_id,
            position_milliseconds,
            observed_at_ms,
        )?,
        PlaybackMutation::FinishActive {
            suppress_auto_advance,
        } => finish_active(transaction, observed_at_ms, suppress_auto_advance)?,
        PlaybackMutation::ReceiptOnly => {}
    }
    Ok(())
}

fn set_rate(transaction: &Transaction<'_>, rate: u16) -> Result<(), StorageError> {
    if !(500..=3_000).contains(&rate) {
        return Err(StorageError::InvalidLegacyRecord {
            entity: "playback",
            index: 0,
            detail: "playback rate is outside supported bounds",
        });
    }
    transaction
        .execute(
            "UPDATE pod0_playback_state SET playback_rate_permille=?1 WHERE singleton=1",
            [rate],
        )
        .map_err(|error| StorageError::sqlite("set playback rate", error))?;
    Ok(())
}

fn set_completion(
    transaction: &Transaction<'_>,
    episode_id: EpisodeId,
    value: CompletionStatus,
) -> Result<(), StorageError> {
    require_episode(transaction, episode_id)?;
    let (code, cause, wire) = completion(&value);
    let reset_position = bool_value(matches!(value, CompletionStatus::Completed { .. }));
    transaction
        .execute(
            "UPDATE pod0_episodes SET completion_code=?1,completion_cause_code=?2,\
             completion_cause_wire_code=?3,resume_position_ms=CASE WHEN ?4=1 THEN 0 \
             ELSE resume_position_ms END WHERE episode_id=?5",
            params![
                code,
                cause,
                wire,
                reset_position,
                episode_id.into_bytes().as_slice()
            ],
        )
        .map_err(|error| StorageError::sqlite("set episode completion", error))?;
    Ok(())
}

fn reset_progress(
    transaction: &Transaction<'_>,
    episode_id: EpisodeId,
) -> Result<(), StorageError> {
    require_episode(transaction, episode_id)?;
    transaction
        .execute(
            "UPDATE pod0_episodes SET resume_position_ms=0 WHERE episode_id=?1",
            [episode_id.into_bytes().as_slice()],
        )
        .map_err(|error| StorageError::sqlite("reset episode progress", error))?;
    transaction
        .execute(
            "UPDATE pod0_playback_state SET last_position_committed_at_ms=NULL WHERE singleton=1",
            [],
        )
        .map_err(|error| StorageError::sqlite("reset playback checkpoint time", error))?;
    Ok(())
}

fn checkpoint(
    transaction: &Transaction<'_>,
    episode_id: EpisodeId,
    position: u64,
    observed_at_ms: i64,
) -> Result<(), StorageError> {
    require_episode(transaction, episode_id)?;
    let duration: Option<i64> = transaction
        .query_row(
            "SELECT duration_ms FROM pod0_episodes WHERE episode_id=?1",
            [episode_id.into_bytes().as_slice()],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("read checkpoint duration", error))?;
    let bounded = duration.map_or(position, |value| position.min(value.max(0) as u64));
    transaction
        .execute(
            "UPDATE pod0_episodes SET resume_position_ms=?1,completion_code=1,\
         completion_cause_code=NULL,completion_cause_wire_code=NULL WHERE episode_id=?2",
            params![
                i64_value(bounded, "resume position")?,
                episode_id.into_bytes().as_slice()
            ],
        )
        .map_err(|error| StorageError::sqlite("checkpoint playback position", error))?;
    transaction
        .execute(
            "UPDATE pod0_playback_state SET last_position_committed_at_ms=?1 WHERE singleton=1",
            [observed_at_ms],
        )
        .map_err(|error| StorageError::sqlite("record playback checkpoint time", error))?;
    Ok(())
}

fn finish_active(
    transaction: &Transaction<'_>,
    observed_at_ms: i64,
    suppress_auto_advance: bool,
) -> Result<(), StorageError> {
    let Some(active) = crate::library_store_playback_support::active_episode(transaction)? else {
        return Ok(());
    };
    let (auto_mark, auto_next, sleep_code): (i64, i64, i64) = transaction
        .query_row(
            "SELECT auto_mark_played_at_natural_end,auto_play_next,sleep_mode_code \
         FROM pod0_playback_state WHERE singleton=1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .map_err(|error| StorageError::sqlite("read completion policy", error))?;
    if auto_mark != 0 {
        let (code, cause, wire) = completion(&CompletionStatus::Completed {
            cause: CompletionCause::NaturalEnd,
        });
        transaction
            .execute(
                "UPDATE pod0_episodes SET resume_position_ms=0,completion_code=?1,\
             completion_cause_code=?2,completion_cause_wire_code=?3 WHERE episode_id=?4",
                params![code, cause, wire, active.into_bytes().as_slice()],
            )
            .map_err(|error| StorageError::sqlite("complete active episode", error))?;
    }
    transaction
        .execute(
            "UPDATE pod0_playback_state SET sleep_mode_code=1,sleep_duration_ms=NULL,\
         sleep_wire_code=NULL,last_position_committed_at_ms=?1 WHERE singleton=1",
            [observed_at_ms],
        )
        .map_err(|error| StorageError::sqlite("finish playback timer", error))?;
    if auto_next != 0 && sleep_code != 3 && !suppress_auto_advance {
        advance_queue(transaction)?;
    }
    Ok(())
}
