use pod0_application::ApplicationCommand;
use sha2::{Digest, Sha256};

use crate::runtime_command_fingerprint_values::{
    hash_clip_source, hash_optional, hash_optional_speaker,
};

pub(super) fn hash_clip_command(hash: &mut Sha256, command: &ApplicationCommand) {
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
        } => {
            hash.update(b"create-clip\0");
            hash.update(clip_id.into_bytes());
            hash.update(episode_id.into_bytes());
            hash.update(podcast_id.into_bytes());
            hash.update(start_milliseconds.to_be_bytes());
            hash.update(end_milliseconds.to_be_bytes());
            hash_optional(hash, caption.as_deref());
            hash_optional_speaker(hash, *speaker_id);
            hash.update(frozen_transcript_text.as_bytes());
            hash.update([0]);
            hash_clip_source(hash, *source);
        }
        ApplicationCommand::UpdateClip {
            clip_id,
            expected_clip_revision,
            start_milliseconds,
            end_milliseconds,
            caption,
            speaker_id,
            frozen_transcript_text,
        } => {
            hash.update(b"update-clip\0");
            hash.update(clip_id.into_bytes());
            hash.update(expected_clip_revision.value.to_be_bytes());
            hash.update(start_milliseconds.to_be_bytes());
            hash.update(end_milliseconds.to_be_bytes());
            hash_optional(hash, caption.as_deref());
            hash_optional_speaker(hash, *speaker_id);
            hash.update(frozen_transcript_text.as_bytes());
            hash.update([0]);
        }
        ApplicationCommand::SetClipDeleted {
            clip_id,
            expected_clip_revision,
            deleted,
        } => {
            hash.update(b"delete-clip\0");
            hash.update(clip_id.into_bytes());
            hash.update(expected_clip_revision.value.to_be_bytes());
            hash.update([u8::from(*deleted)]);
        }
        ApplicationCommand::ClearClips {
            expected_collection_revision,
        } => {
            hash.update(b"clear-clips\0");
            hash.update(expected_collection_revision.value.to_be_bytes());
        }
        _ => unreachable!("clip fingerprint helper received a non-clip command"),
    }
}
