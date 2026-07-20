use pod0_application::{
    ChapterPlaybackContext, CommandEnvelope, CoreFailureCode, HostRequest, OperationResult,
    PlaybackHostState,
};
use pod0_domain::{
    CHAPTER_PLAYBACK_POLICY_VERSION, ChapterNavigationDirection, ChapterPlaybackSessionId,
    CommandId, PlaybackSeekReason, decide_automatic_ad_skip, decide_chapter_navigation,
};
use pod0_storage::PlaybackMutation;
use sha2::{Digest, Sha256};

use crate::runtime_playback_state::ActiveChapterPlayback;
use crate::runtime_state::FacadeState;
use crate::runtime_storage_commands::storage_failure;

impl FacadeState {
    pub(super) fn sync_active_chapter(
        &mut self,
        session_command_id: CommandId,
    ) -> Result<(), pod0_storage::StorageError> {
        let Some(episode_id) = self.listening.playback.active_episode_id else {
            self.clear_active_chapter();
            return Ok(());
        };
        let Some(store) = self.store.as_ref() else {
            self.clear_active_chapter();
            return Err(pod0_storage::StorageError::CutoverNotAuthoritative);
        };
        let selected = store.selected_chapter_artifact(episode_id);
        let selected = match selected {
            Ok(selected) => selected,
            Err(error) => {
                self.clear_active_chapter();
                return Err(error);
            }
        };
        let Some(selected) = selected else {
            self.clear_active_chapter();
            return Ok(());
        };
        let unchanged = self.playback.chapter.as_ref().is_some_and(|active| {
            active.context.episode_id == episode_id
                && active.context.artifact_id == selected.artifact.artifact_id
                && active.context.selection_revision == selected.selection_revision
        });
        if unchanged {
            return Ok(());
        }
        let context = ChapterPlaybackContext {
            episode_id,
            artifact_id: selected.artifact.artifact_id,
            selection_revision: selected.selection_revision,
            session_id: chapter_session_id(
                session_command_id,
                episode_id,
                selected.artifact.artifact_id,
                selected.selection_revision,
            ),
            policy_version: CHAPTER_PLAYBACK_POLICY_VERSION,
        };
        self.playback.chapter = Some(ActiveChapterPlayback {
            context,
            artifact: selected.artifact,
        });
        self.playback.skipped_ad_span_ids.clear();
        Ok(())
    }

    pub(super) fn navigate_chapter(
        &mut self,
        envelope: &CommandEnvelope,
        fingerprint: &str,
        supplied_context: ChapterPlaybackContext,
        position_milliseconds: u64,
        direction: ChapterNavigationDirection,
    ) {
        if let Err(error) = self.sync_active_chapter(envelope.command_id) {
            self.fail(envelope.command_id, storage_failure(error));
            return;
        }
        let Some(active) = self.playback.chapter.as_ref() else {
            self.fail(envelope.command_id, CoreFailureCode::NotFound);
            return;
        };
        if active.context != supplied_context
            || supplied_context.policy_version != CHAPTER_PLAYBACK_POLICY_VERSION
        {
            self.fail(envelope.command_id, CoreFailureCode::RevisionConflict);
            return;
        }
        let decision =
            decide_chapter_navigation(&active.artifact, position_milliseconds, direction);
        let mutation = decision.map_or(PlaybackMutation::ReceiptOnly, |value| {
            PlaybackMutation::Checkpoint {
                episode_id: supplied_context.episode_id,
                position_milliseconds: value.target_milliseconds,
            }
        });
        let Some(is_new) = self.commit_chapter_action(envelope, fingerprint, mutation) else {
            return;
        };
        let Some(decision) = decision.filter(|_| is_new) else {
            return;
        };
        let command_at_ms = self.now().value;
        self.playback.position_command_fence_at_ms = Some(command_at_ms);
        self.playback.last_position_commit_at_ms = Some(command_at_ms);
        self.issue_playback_request(
            envelope,
            "chapter-seek",
            HostRequest::Seek {
                episode_id: supplied_context.episode_id,
                position_milliseconds: decision.target_milliseconds,
                reason: decision.reason,
                chapter_context: Some(supplied_context),
            },
        );
    }

    pub(super) fn evaluate_automatic_ad_skip(
        &mut self,
        reaction: &CommandEnvelope,
        observed_at_ms: i64,
        position_milliseconds: u64,
    ) -> bool {
        let Some(active) = self.playback.chapter.as_ref() else {
            return false;
        };
        let decision = decide_automatic_ad_skip(
            &active.artifact,
            position_milliseconds,
            self.playback.auto_skip_ads,
            self.playback.host_state == PlaybackHostState::Playing,
            &self.playback.skipped_ad_span_ids,
        );
        let Some(decision) = decision else {
            return false;
        };
        let Some(ad_span_id) = decision.ad_span_id else {
            return false;
        };
        let context = active.context;
        let committed = self
            .store
            .as_ref()
            .ok_or(pod0_storage::StorageError::CutoverNotAuthoritative)
            .and_then(|store| {
                store.apply_playback_observation(
                    PlaybackMutation::Checkpoint {
                        episode_id: context.episode_id,
                        position_milliseconds: decision.target_milliseconds,
                    },
                    observed_at_ms,
                )
            });
        if committed.is_err() || self.reload_listening().is_err() {
            self.playback.policy_state = pod0_application::PlaybackPolicyState::Failed;
            return true;
        }
        self.playback.skipped_ad_span_ids.insert(ad_span_id);
        self.playback.position_command_fence_at_ms = Some(observed_at_ms);
        self.playback.last_position_commit_at_ms = Some(observed_at_ms);
        self.issue_playback_request(
            reaction,
            "automatic-ad-skip",
            HostRequest::Seek {
                episode_id: context.episode_id,
                position_milliseconds: decision.target_milliseconds,
                reason: PlaybackSeekReason::AutomaticAdSkip,
                chapter_context: Some(context),
            },
        );
        true
    }

    fn commit_chapter_action(
        &mut self,
        envelope: &CommandEnvelope,
        fingerprint: &str,
        mutation: PlaybackMutation,
    ) -> Option<bool> {
        let outcome = self
            .store
            .as_ref()
            .ok_or(pod0_storage::StorageError::CutoverNotAuthoritative)
            .and_then(|store| {
                store.apply_playback_mutation(
                    envelope.command_id,
                    fingerprint,
                    mutation,
                    self.now().value,
                )
            });
        match outcome {
            Ok(result) => match self.reload_listening() {
                Ok(()) => {
                    self.succeed(
                        envelope.command_id,
                        Some(OperationResult::PlaybackUpdated {
                            episode_id: self.listening.playback.active_episode_id,
                        }),
                    );
                    Some(!result.reused_existing)
                }
                Err(error) => {
                    self.fail(envelope.command_id, storage_failure(error));
                    None
                }
            },
            Err(error) => {
                self.fail(envelope.command_id, storage_failure(error));
                None
            }
        }
    }

    fn clear_active_chapter(&mut self) {
        self.playback.chapter = None;
        self.playback.skipped_ad_span_ids.clear();
    }
}

fn chapter_session_id(
    command_id: CommandId,
    episode_id: pod0_domain::EpisodeId,
    artifact_id: pod0_domain::ChapterArtifactId,
    selection_revision: pod0_domain::StateRevision,
) -> ChapterPlaybackSessionId {
    let mut hash = Sha256::new();
    hash.update(b"pod0-chapter-playback-session-v1\0");
    hash.update(command_id.into_bytes());
    hash.update(episode_id.into_bytes());
    hash.update(artifact_id.into_bytes());
    hash.update(selection_revision.value.to_be_bytes());
    let digest = hash.finalize();
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    ChapterPlaybackSessionId::from_bytes(bytes)
}
