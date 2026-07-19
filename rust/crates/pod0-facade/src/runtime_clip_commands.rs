use pod0_application::{CommandEnvelope, OperationResult};

use crate::runtime_state::FacadeState;
use crate::runtime_storage_commands::storage_failure;

impl FacadeState {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn create_clip(
        &mut self,
        envelope: &CommandEnvelope,
        fingerprint: &str,
        clip_id: pod0_domain::ClipId,
        episode_id: pod0_domain::EpisodeId,
        podcast_id: pod0_domain::PodcastId,
        start_milliseconds: u64,
        end_milliseconds: u64,
        caption: Option<&str>,
        speaker_id: Option<pod0_domain::SpeakerId>,
        frozen_transcript_text: &str,
        source: pod0_domain::ClipSource,
    ) {
        let result = self
            .store
            .as_ref()
            .ok_or(pod0_storage::StorageError::CutoverNotAuthoritative)
            .and_then(|store| {
                store.create_clip(
                    envelope.command_id,
                    fingerprint,
                    clip_id,
                    episode_id,
                    podcast_id,
                    start_milliseconds,
                    end_milliseconds,
                    caption,
                    speaker_id,
                    frozen_transcript_text,
                    source,
                    self.now().value,
                )
            });
        match result {
            Ok(collection_revision) => self.finish_clip_change(
                envelope.command_id,
                clip_id,
                pod0_domain::ClipRevision::INITIAL,
                collection_revision,
                true,
            ),
            Err(error) => self.fail(envelope.command_id, storage_failure(error)),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn update_clip(
        &mut self,
        envelope: &CommandEnvelope,
        fingerprint: &str,
        clip_id: pod0_domain::ClipId,
        expected_revision: pod0_domain::ClipRevision,
        start_milliseconds: u64,
        end_milliseconds: u64,
        caption: Option<&str>,
        speaker_id: Option<pod0_domain::SpeakerId>,
        frozen_transcript_text: &str,
    ) {
        let result = self
            .store
            .as_ref()
            .ok_or(pod0_storage::StorageError::CutoverNotAuthoritative)
            .and_then(|store| {
                store.update_clip(
                    envelope.command_id,
                    fingerprint,
                    clip_id,
                    expected_revision,
                    start_milliseconds,
                    end_milliseconds,
                    caption,
                    speaker_id,
                    frozen_transcript_text,
                    self.now().value,
                )
            });
        match result {
            Ok(collection_revision) => self.finish_clip_change(
                envelope.command_id,
                clip_id,
                pod0_domain::ClipRevision::new(expected_revision.value.saturating_add(1)),
                collection_revision,
                false,
            ),
            Err(error) => self.fail(envelope.command_id, storage_failure(error)),
        }
    }

    pub(super) fn set_clip_deleted(
        &mut self,
        envelope: &CommandEnvelope,
        fingerprint: &str,
        clip_id: pod0_domain::ClipId,
        expected_revision: pod0_domain::ClipRevision,
        deleted: bool,
    ) {
        let result = self
            .store
            .as_ref()
            .ok_or(pod0_storage::StorageError::CutoverNotAuthoritative)
            .and_then(|store| {
                store.set_clip_deleted(
                    envelope.command_id,
                    fingerprint,
                    clip_id,
                    expected_revision,
                    deleted,
                    self.now().value,
                )
            });
        match result {
            Ok(collection_revision) => self.finish_clip_change(
                envelope.command_id,
                clip_id,
                pod0_domain::ClipRevision::new(expected_revision.value.saturating_add(1)),
                collection_revision,
                false,
            ),
            Err(error) => self.fail(envelope.command_id, storage_failure(error)),
        }
    }

    pub(super) fn clear_clips(
        &mut self,
        envelope: &CommandEnvelope,
        fingerprint: &str,
        expected_collection_revision: pod0_domain::StateRevision,
    ) {
        let result = self
            .store
            .as_ref()
            .ok_or(pod0_storage::StorageError::CutoverNotAuthoritative)
            .and_then(|store| {
                store.clear_clips(
                    envelope.command_id,
                    fingerprint,
                    expected_collection_revision,
                    self.now().value,
                )
            });
        match result {
            Ok(collection_revision) => {
                self.finish_clip_clear(envelope.command_id, collection_revision)
            }
            Err(error) => self.fail(envelope.command_id, storage_failure(error)),
        }
    }

    fn finish_clip_change(
        &mut self,
        command_id: pod0_domain::CommandId,
        clip_id: pod0_domain::ClipId,
        clip_revision: pod0_domain::ClipRevision,
        collection_revision: pod0_domain::StateRevision,
        created: bool,
    ) {
        match self.reload_clips() {
            Ok(()) => {
                let result = if created {
                    OperationResult::ClipCreated {
                        clip_id,
                        clip_revision,
                        collection_revision,
                    }
                } else {
                    OperationResult::ClipUpdated {
                        clip_id,
                        clip_revision,
                        collection_revision,
                    }
                };
                self.succeed(command_id, Some(result));
            }
            Err(error) => self.fail(command_id, storage_failure(error)),
        }
    }

    fn finish_clip_clear(
        &mut self,
        command_id: pod0_domain::CommandId,
        collection_revision: pod0_domain::StateRevision,
    ) {
        match self.reload_clips() {
            Ok(()) => self.succeed(
                command_id,
                Some(OperationResult::ClipsCleared {
                    collection_revision,
                }),
            ),
            Err(error) => self.fail(command_id, storage_failure(error)),
        }
    }

    pub(super) fn reload_clips(&mut self) -> Result<(), pod0_storage::StorageError> {
        if let Some(store) = &self.store {
            let clips = store.clip_snapshot()?;
            self.revision =
                pod0_domain::StateRevision::new(self.revision.value.max(clips.revision.value));
            self.clips = clips;
        }
        Ok(())
    }
}
