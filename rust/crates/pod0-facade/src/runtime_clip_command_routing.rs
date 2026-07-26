use pod0_application::{ApplicationCommand, CommandEnvelope};

use crate::runtime_state::FacadeState;

impl FacadeState {
    /// Clip arms of the command match, split out of `runtime_commands.rs`.
    pub(super) fn route_clip_command(
        &mut self,
        envelope: &CommandEnvelope,
        fingerprint: &str,
        command: ApplicationCommand,
    ) {
        match command {
            ApplicationCommand::CreateClip {
                clip_id,
                episode_id,
                podcast_id,
                start_milliseconds,
                end_milliseconds,
                caption,
                speaker_id,
                frozen_transcript_text,
                source,
            } => self.create_clip(
                envelope,
                fingerprint,
                clip_id,
                episode_id,
                podcast_id,
                start_milliseconds,
                end_milliseconds,
                caption.as_deref(),
                speaker_id,
                &frozen_transcript_text,
                source,
            ),
            ApplicationCommand::UpdateClip {
                clip_id,
                expected_clip_revision,
                start_milliseconds,
                end_milliseconds,
                caption,
                speaker_id,
                frozen_transcript_text,
            } => self.update_clip(
                envelope,
                fingerprint,
                clip_id,
                expected_clip_revision,
                start_milliseconds,
                end_milliseconds,
                caption.as_deref(),
                speaker_id,
                &frozen_transcript_text,
            ),
            ApplicationCommand::SetClipDeleted {
                clip_id,
                expected_clip_revision,
                deleted,
            } => self.set_clip_deleted(
                envelope,
                fingerprint,
                clip_id,
                expected_clip_revision,
                deleted,
            ),
            ApplicationCommand::ClearClips {
                expected_collection_revision,
            } => self.clear_clips(envelope, fingerprint, expected_collection_revision),
            _ => unreachable!("only clip commands are routed here"),
        }
    }
}
