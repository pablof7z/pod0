use pod0_domain::{
    ClipId, ClipRevision, ClipSource, CommandId, EpisodeId, PodcastId, SpeakerId, StateRevision,
};

use crate::StorageError;
use crate::library_store::LibraryStore;

impl LibraryStore {
    #[allow(clippy::too_many_arguments)]
    pub fn create_clip(
        &self,
        command_id: CommandId,
        command_fingerprint: &str,
        clip_id: ClipId,
        episode_id: EpisodeId,
        podcast_id: PodcastId,
        start_milliseconds: u64,
        end_milliseconds: u64,
        caption: Option<&str>,
        speaker_id: Option<SpeakerId>,
        frozen_transcript_text: &str,
        source: ClipSource,
        observed_at_ms: i64,
    ) -> Result<StateRevision, StorageError> {
        crate::transition_commit::commit_clip_create(
            self.path(),
            command_id,
            command_fingerprint,
            clip_id,
            episode_id,
            podcast_id,
            start_milliseconds,
            end_milliseconds,
            caption,
            speaker_id,
            frozen_transcript_text,
            source,
            observed_at_ms,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn update_clip(
        &self,
        command_id: CommandId,
        command_fingerprint: &str,
        clip_id: ClipId,
        expected_revision: ClipRevision,
        start_milliseconds: u64,
        end_milliseconds: u64,
        caption: Option<&str>,
        speaker_id: Option<SpeakerId>,
        frozen_transcript_text: &str,
        observed_at_ms: i64,
    ) -> Result<StateRevision, StorageError> {
        crate::transition_commit::commit_clip_update(
            self.path(),
            command_id,
            command_fingerprint,
            clip_id,
            expected_revision,
            start_milliseconds,
            end_milliseconds,
            caption,
            speaker_id,
            frozen_transcript_text,
            observed_at_ms,
        )
    }

    pub fn set_clip_deleted(
        &self,
        command_id: CommandId,
        command_fingerprint: &str,
        clip_id: ClipId,
        expected_revision: ClipRevision,
        deleted: bool,
        observed_at_ms: i64,
    ) -> Result<StateRevision, StorageError> {
        crate::transition_commit::commit_clip_deleted(
            self.path(),
            command_id,
            command_fingerprint,
            clip_id,
            expected_revision,
            deleted,
            observed_at_ms,
        )
    }

    pub fn clear_clips(
        &self,
        command_id: CommandId,
        command_fingerprint: &str,
        expected_collection_revision: StateRevision,
        observed_at_ms: i64,
    ) -> Result<StateRevision, StorageError> {
        crate::transition_commit::commit_clip_clear(
            self.path(),
            command_id,
            command_fingerprint,
            expected_collection_revision,
            observed_at_ms,
        )
    }
}
