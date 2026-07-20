use pod0_application::{
    CommandEnvelope, CoreFailureCode, HostRequest, OperationResult, PlaybackCommand,
    PlaybackTransitionCue, QueuePlacement,
};
use pod0_domain::ChapterNavigationDirection;
use pod0_storage::{PlaybackMutation, PlaybackQueuePlacement};

use crate::runtime_state::FacadeState;

impl FacadeState {
    pub(super) fn accept_playback_command(
        &mut self,
        envelope: &CommandEnvelope,
        fingerprint: &str,
        command: PlaybackCommand,
    ) {
        match command {
            PlaybackCommand::Select {
                episode_id,
                segment,
                label,
            } => {
                let selection_is_unchanged = self.listening.playback.active_episode_id
                    == Some(episode_id)
                    && self.listening.playback.active_segment == segment
                    && self.listening.playback.active_label == label;
                let media_is_reusable = self.playback.media_episode_id == Some(episode_id)
                    && self
                        .playback
                        .last_observation
                        .as_ref()
                        .is_none_or(|value| !value.ended)
                    && !matches!(
                        self.playback.host_state,
                        pod0_application::PlaybackHostState::Failed
                            | pod0_application::PlaybackHostState::Unsupported { .. }
                    );
                if self.apply_playback_command(
                    envelope,
                    fingerprint,
                    PlaybackMutation::Select {
                        episode_id,
                        segment,
                        label,
                    },
                    OperationResult::PlaybackUpdated {
                        episode_id: Some(episode_id),
                    },
                ) {
                    if selection_is_unchanged && media_is_reusable {
                        self.ensure_playback_stream(envelope);
                    } else {
                        self.playback.desired_playing = false;
                        self.playback.timer_fired = false;
                        self.load_active(envelope, false, PlaybackTransitionCue::Immediate);
                    }
                }
            }
            PlaybackCommand::Restore => {
                if self.apply_playback_command(
                    envelope,
                    fingerprint,
                    PlaybackMutation::ReceiptOnly,
                    OperationResult::PlaybackUpdated {
                        episode_id: self.listening.playback.active_episode_id,
                    },
                ) {
                    self.playback.desired_playing = false;
                    self.load_active(envelope, false, PlaybackTransitionCue::Immediate);
                }
            }
            PlaybackCommand::Play => self.play(envelope, fingerprint),
            PlaybackCommand::Pause => self.pause(envelope, fingerprint),
            PlaybackCommand::Seek {
                position_milliseconds,
            } => self.seek(envelope, fingerprint, position_milliseconds),
            PlaybackCommand::NextChapter {
                context,
                position_milliseconds,
            } => self.navigate_chapter(
                envelope,
                fingerprint,
                context,
                position_milliseconds,
                ChapterNavigationDirection::Next,
            ),
            PlaybackCommand::PreviousChapter {
                context,
                position_milliseconds,
            } => self.navigate_chapter(
                envelope,
                fingerprint,
                context,
                position_milliseconds,
                ChapterNavigationDirection::Previous,
            ),
            PlaybackCommand::Enqueue { entry, placement } => {
                let placement = match placement {
                    QueuePlacement::Back => PlaybackQueuePlacement::Back,
                    QueuePlacement::Next => PlaybackQueuePlacement::Next,
                    QueuePlacement::Unsupported { wire_code } => {
                        self.fail(
                            envelope.command_id,
                            CoreFailureCode::Unsupported { wire_code },
                        );
                        return;
                    }
                };
                self.apply_playback_command(
                    envelope,
                    fingerprint,
                    PlaybackMutation::Enqueue { entry, placement },
                    OperationResult::QueueUpdated,
                );
            }
            PlaybackCommand::RemoveQueueEntry { queue_entry_id } => {
                self.apply_playback_command(
                    envelope,
                    fingerprint,
                    PlaybackMutation::RemoveQueueEntry(queue_entry_id),
                    OperationResult::QueueUpdated,
                );
            }
            PlaybackCommand::RemoveEpisodeFromQueue { episode_id } => {
                self.apply_playback_command(
                    envelope,
                    fingerprint,
                    PlaybackMutation::RemoveEpisode(episode_id),
                    OperationResult::QueueUpdated,
                );
            }
            PlaybackCommand::ReplaceQueueOrder { queue_entry_ids } => {
                self.apply_playback_command(
                    envelope,
                    fingerprint,
                    PlaybackMutation::ReplaceQueueOrder(queue_entry_ids),
                    OperationResult::QueueUpdated,
                );
            }
            PlaybackCommand::ClearQueue => {
                self.apply_playback_command(
                    envelope,
                    fingerprint,
                    PlaybackMutation::ClearQueue,
                    OperationResult::QueueUpdated,
                );
            }
            PlaybackCommand::AdvanceQueue => self.advance_queue(envelope, fingerprint),
            PlaybackCommand::SetRate { rate } => {
                if self.apply_playback_command(
                    envelope,
                    fingerprint,
                    PlaybackMutation::SetRate(rate),
                    OperationResult::PlaybackUpdated {
                        episode_id: self.listening.playback.active_episode_id,
                    },
                ) && let Some(episode_id) = self.listening.playback.active_episode_id
                {
                    self.issue_playback_request(
                        envelope,
                        "rate",
                        HostRequest::SetRate { episode_id, rate },
                    );
                }
            }
            PlaybackCommand::SetSleepTimer { mode } => {
                self.set_sleep_timer(envelope, fingerprint, mode);
            }
            PlaybackCommand::SetPreferences {
                auto_mark_played_at_natural_end,
                auto_play_next,
                auto_skip_ads,
            } => {
                if self.apply_playback_command(
                    envelope,
                    fingerprint,
                    PlaybackMutation::SetPreferences {
                        auto_mark_played_at_natural_end,
                        auto_play_next,
                    },
                    OperationResult::PlaybackUpdated {
                        episode_id: self.listening.playback.active_episode_id,
                    },
                ) {
                    self.playback.auto_skip_ads = auto_skip_ads;
                }
            }
            PlaybackCommand::SetCompletion {
                episode_id,
                completion,
            } => {
                self.apply_playback_command(
                    envelope,
                    fingerprint,
                    PlaybackMutation::SetCompletion {
                        episode_id,
                        completion,
                    },
                    OperationResult::PlaybackUpdated {
                        episode_id: Some(episode_id),
                    },
                );
            }
            PlaybackCommand::ResetProgress { episode_id } => {
                self.apply_playback_command(
                    envelope,
                    fingerprint,
                    PlaybackMutation::ResetProgress(episode_id),
                    OperationResult::PlaybackUpdated {
                        episode_id: Some(episode_id),
                    },
                );
            }
            PlaybackCommand::Checkpoint {
                episode_id,
                position_milliseconds,
            } => {
                let command_at_ms = self.now().value;
                if self.apply_playback_command(
                    envelope,
                    fingerprint,
                    PlaybackMutation::Checkpoint {
                        episode_id,
                        position_milliseconds,
                    },
                    OperationResult::PlaybackUpdated {
                        episode_id: Some(episode_id),
                    },
                ) {
                    self.playback.position_command_fence_at_ms = Some(command_at_ms);
                    self.playback.last_position_commit_at_ms = Some(command_at_ms);
                }
            }
            PlaybackCommand::NativeTimerFired => self.timer_fired(envelope, fingerprint),
        }
    }
}
