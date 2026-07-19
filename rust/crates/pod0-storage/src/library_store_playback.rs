use pod0_domain::{
    CommandId, CompletionStatus, EpisodeId, PlaybackRatePermille, PlaybackSegment,
    PlaybackSleepMode, QueueEntry, QueueEntryId, StateRevision,
};

use crate::StorageError;
use crate::library_store::{LibraryStore, command_was_applied, finish_command};
use crate::library_store_playback_apply::apply_mutation;
use crate::library_store_playback_support::{active_episode, advance_revision, current_revision};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaybackQueuePlacement {
    Back,
    Next,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlaybackMutation {
    Select {
        episode_id: EpisodeId,
        segment: Option<PlaybackSegment>,
        label: Option<String>,
    },
    Enqueue {
        entry: QueueEntry,
        placement: PlaybackQueuePlacement,
    },
    RemoveQueueEntry(QueueEntryId),
    RemoveEpisode(EpisodeId),
    ReplaceQueueOrder(Vec<QueueEntryId>),
    ClearQueue,
    AdvanceQueue,
    SetRate(PlaybackRatePermille),
    SetSleepTimer(PlaybackSleepMode),
    SetPreferences {
        auto_mark_played_at_natural_end: bool,
        auto_play_next: bool,
    },
    SetCompletion {
        episode_id: EpisodeId,
        completion: CompletionStatus,
    },
    ResetProgress(EpisodeId),
    Checkpoint {
        episode_id: EpisodeId,
        position_milliseconds: u64,
    },
    FinishActive {
        suppress_auto_advance: bool,
    },
    ReceiptOnly,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlaybackMutationResult {
    pub revision: StateRevision,
    pub active_episode_id: Option<EpisodeId>,
}

impl LibraryStore {
    pub fn apply_playback_mutation(
        &self,
        command_id: CommandId,
        fingerprint: &str,
        mutation: PlaybackMutation,
        observed_at_ms: i64,
    ) -> Result<PlaybackMutationResult, StorageError> {
        self.write(|transaction| {
            if let Some(revision) = command_was_applied(transaction, command_id, fingerprint)? {
                return Ok(PlaybackMutationResult {
                    revision,
                    active_episode_id: active_episode(transaction)?,
                });
            }
            apply_mutation(transaction, mutation, observed_at_ms)?;
            let revision = finish_command(transaction, command_id, fingerprint, observed_at_ms)?;
            Ok(PlaybackMutationResult {
                revision,
                active_episode_id: active_episode(transaction)?,
            })
        })
    }

    pub fn clear_session_sleep_timer(&self) -> Result<StateRevision, StorageError> {
        self.write(|transaction| {
            let mode: i64 = transaction
                .query_row(
                    "SELECT sleep_mode_code FROM pod0_playback_state WHERE singleton=1",
                    [],
                    |row| row.get(0),
                )
                .map_err(|error| StorageError::sqlite("read session sleep timer", error))?;
            if mode == 1 {
                current_revision(transaction)
            } else {
                transaction
                    .execute(
                        "UPDATE pod0_playback_state SET sleep_mode_code=1,sleep_duration_ms=NULL,\
                         sleep_wire_code=NULL WHERE singleton=1",
                        [],
                    )
                    .map_err(|error| StorageError::sqlite("clear session sleep timer", error))?;
                advance_revision(transaction)
            }
        })
    }

    /// Accepted host observations bypass the durable user-command receipt
    /// table, keeping thirty-second playback checkpoints bounded on disk.
    pub fn apply_playback_observation(
        &self,
        mutation: PlaybackMutation,
        observed_at_ms: i64,
    ) -> Result<PlaybackMutationResult, StorageError> {
        self.write(|transaction| {
            apply_mutation(transaction, mutation, observed_at_ms)?;
            let revision = advance_revision(transaction)?;
            Ok(PlaybackMutationResult {
                revision,
                active_episode_id: active_episode(transaction)?,
            })
        })
    }
}
