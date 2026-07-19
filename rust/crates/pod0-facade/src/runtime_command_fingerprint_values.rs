use pod0_application::{PlaybackCommand, QueuePlacement};
use pod0_domain::{
    AutoDownloadMode, AutoDownloadPolicy, CompletionCause, CompletionStatus, PlaybackSegment,
    PlaybackSleepMode, QueueEntry,
};
use sha2::{Digest, Sha256};

pub(super) fn hash_playback(hash: &mut Sha256, command: &PlaybackCommand) {
    match command {
        PlaybackCommand::Select {
            episode_id,
            segment,
            label,
        } => {
            hash.update(b"playback-select\0");
            hash.update(episode_id.into_bytes());
            hash_segment(hash, *segment);
            hash_optional(hash, label.as_deref());
        }
        PlaybackCommand::Restore => hash.update(b"playback-restore\0"),
        PlaybackCommand::Play => hash.update(b"playback-play\0"),
        PlaybackCommand::Pause => hash.update(b"playback-pause\0"),
        PlaybackCommand::Seek {
            position_milliseconds,
        } => {
            hash.update(b"playback-seek\0");
            hash.update(position_milliseconds.to_be_bytes());
        }
        PlaybackCommand::Enqueue { entry, placement } => {
            hash.update(b"playback-enqueue\0");
            hash_queue_entry(hash, entry);
            match placement {
                QueuePlacement::Back => hash.update([1]),
                QueuePlacement::Next => hash.update([2]),
                QueuePlacement::Unsupported { wire_code } => {
                    hash.update([255]);
                    hash.update(wire_code.to_be_bytes());
                }
            }
        }
        PlaybackCommand::RemoveQueueEntry { queue_entry_id } => {
            hash.update(b"playback-remove-entry\0");
            hash.update(queue_entry_id.into_bytes());
        }
        PlaybackCommand::RemoveEpisodeFromQueue { episode_id } => {
            hash.update(b"playback-remove-episode\0");
            hash.update(episode_id.into_bytes());
        }
        PlaybackCommand::ReplaceQueueOrder { queue_entry_ids } => {
            hash.update(b"playback-reorder\0");
            hash.update((queue_entry_ids.len() as u64).to_be_bytes());
            for id in queue_entry_ids {
                hash.update(id.into_bytes());
            }
        }
        PlaybackCommand::ClearQueue => hash.update(b"playback-clear-queue\0"),
        PlaybackCommand::AdvanceQueue => hash.update(b"playback-advance\0"),
        PlaybackCommand::SetRate { rate } => {
            hash.update(b"playback-rate\0");
            hash.update(rate.value.to_be_bytes());
        }
        PlaybackCommand::SetSleepTimer { mode } => {
            hash.update(b"playback-sleep\0");
            hash_sleep(hash, *mode);
        }
        PlaybackCommand::SetPreferences {
            auto_mark_played_at_natural_end,
            auto_play_next,
        } => {
            hash.update(b"playback-preferences\0");
            hash.update([
                u8::from(*auto_mark_played_at_natural_end),
                u8::from(*auto_play_next),
            ]);
        }
        PlaybackCommand::SetCompletion {
            episode_id,
            completion,
        } => {
            hash.update(b"playback-completion\0");
            hash.update(episode_id.into_bytes());
            hash_completion(hash, *completion);
        }
        PlaybackCommand::ResetProgress { episode_id } => {
            hash.update(b"playback-reset-progress\0");
            hash.update(episode_id.into_bytes());
        }
        PlaybackCommand::Checkpoint {
            episode_id,
            position_milliseconds,
        } => {
            hash.update(b"playback-checkpoint\0");
            hash.update(episode_id.into_bytes());
            hash.update(position_milliseconds.to_be_bytes());
        }
        PlaybackCommand::NativeTimerFired => hash.update(b"playback-timer-fired\0"),
    }
}

pub(super) fn hash_optional(hash: &mut Sha256, value: Option<&str>) {
    match value {
        Some(value) => {
            hash.update([1]);
            hash.update(value.as_bytes());
        }
        None => hash.update([0]),
    }
    hash.update([0]);
}

pub(super) fn hash_policy(hash: &mut Sha256, policy: &AutoDownloadPolicy) {
    match policy.mode {
        AutoDownloadMode::Off => hash.update([1]),
        AutoDownloadMode::Latest { count } => {
            hash.update([2]);
            hash.update(count.to_be_bytes());
        }
        AutoDownloadMode::AllNew => hash.update([3]),
        AutoDownloadMode::Unsupported { wire_code } => {
            hash.update([255]);
            hash.update(wire_code.to_be_bytes());
        }
    }
    hash.update([u8::from(policy.wifi_only)]);
}

fn hash_queue_entry(hash: &mut Sha256, entry: &QueueEntry) {
    hash.update(entry.queue_entry_id.into_bytes());
    hash.update(entry.episode_id.into_bytes());
    hash_segment(hash, entry.segment);
    hash_optional(hash, entry.label.as_deref());
}

fn hash_segment(hash: &mut Sha256, segment: Option<PlaybackSegment>) {
    match segment {
        Some(value) => {
            hash.update([1]);
            hash_optional_u64(hash, value.start_position_milliseconds);
            hash_optional_u64(hash, value.end_position_milliseconds);
        }
        None => hash.update([0]),
    }
}

fn hash_optional_u64(hash: &mut Sha256, value: Option<u64>) {
    match value {
        Some(value) => {
            hash.update([1]);
            hash.update(value.to_be_bytes());
        }
        None => hash.update([0]),
    }
}

fn hash_sleep(hash: &mut Sha256, mode: PlaybackSleepMode) {
    match mode {
        PlaybackSleepMode::Off => hash.update([1]),
        PlaybackSleepMode::Duration {
            duration_milliseconds,
        } => {
            hash.update([2]);
            hash.update(duration_milliseconds.to_be_bytes());
        }
        PlaybackSleepMode::EndOfEpisode => hash.update([3]),
        PlaybackSleepMode::Unsupported { wire_code } => {
            hash.update([255]);
            hash.update(wire_code.to_be_bytes());
        }
    }
}

fn hash_completion(hash: &mut Sha256, completion: CompletionStatus) {
    match completion {
        CompletionStatus::InProgress => hash.update([1]),
        CompletionStatus::Completed { cause } => {
            hash.update([2]);
            match cause {
                CompletionCause::NaturalEnd => hash.update([1]),
                CompletionCause::ExplicitUserAction => hash.update([2]),
                CompletionCause::LegacyPlayedFlag => hash.update([3]),
                CompletionCause::Unsupported { wire_code } => {
                    hash.update([255]);
                    hash.update(wire_code.to_be_bytes());
                }
            }
        }
        CompletionStatus::Unsupported { wire_code } => {
            hash.update([255]);
            hash.update(wire_code.to_be_bytes());
        }
    }
}
