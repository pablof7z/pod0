use pod0_application::PlaybackTransition;
use pod0_domain::{
    CommandId, CompletionStatus, EpisodeId, PlaybackRatePermille, PlaybackSegment,
    PlaybackSleepMode, QueueEntry, QueueEntryId, StateRevision,
};

use crate::StorageError;
use crate::library_store::LibraryStore;

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
    CheckpointAndAdvanceQueue {
        episode_id: EpisodeId,
        position_milliseconds: u64,
    },
    CheckpointAndFinishActive {
        episode_id: EpisodeId,
        position_milliseconds: u64,
        suppress_auto_advance: bool,
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
    pub reused_existing: bool,
}

impl LibraryStore {
    #[allow(clippy::too_many_arguments)]
    pub fn apply_playback_mutation(
        &self,
        command_id: CommandId,
        fingerprint: &str,
        mutation: PlaybackMutation,
        episode_id: Option<EpisodeId>,
        transition: PlaybackTransition,
        internal_command: Option<pod0_application::DurableInternalCommandRequest>,
        effects: Vec<pod0_application::DurablePlaybackEffectRequest>,
        observed_at_ms: i64,
    ) -> Result<PlaybackMutationResult, StorageError> {
        crate::transition_commit::commit_playback_mutation(
            self.path(),
            command_id,
            fingerprint,
            mutation,
            episode_id,
            transition,
            internal_command,
            effects,
            observed_at_ms,
        )
    }

    pub fn clear_session_sleep_timer(
        &self,
        observed_at_ms: i64,
    ) -> Result<StateRevision, StorageError> {
        let snapshot = self.snapshot()?;
        if snapshot.playback.sleep_mode == PlaybackSleepMode::Off {
            return Ok(snapshot.playback.revision);
        }
        let command_id = recovery_sleep_timer_command(snapshot.playback.revision);
        self.apply_playback_mutation(
            command_id,
            &hex_digest(command_id.into_bytes()),
            PlaybackMutation::SetSleepTimer(PlaybackSleepMode::Off),
            snapshot.playback.active_episode_id,
            PlaybackTransition::SleepTimerChanged,
            None,
            Vec::new(),
            observed_at_ms,
        )
        .map(|result| result.revision)
    }
}

fn recovery_sleep_timer_command(revision: StateRevision) -> CommandId {
    use sha2::{Digest as _, Sha256};
    let mut hash = Sha256::new();
    hash.update(b"pod0/playback/recovery-clear-sleep/v1");
    hash.update(revision.value.to_be_bytes());
    CommandId::from_bytes(
        hash.finalize()[..16]
            .try_into()
            .expect("fixed digest prefix"),
    )
}

fn hex_digest(value: [u8; 16]) -> String {
    use std::fmt::Write as _;
    let mut output = String::with_capacity(64);
    for byte in value.into_iter().chain(value) {
        write!(&mut output, "{byte:02x}").expect("writing to String cannot fail");
    }
    output
}
