use pod0_domain::{
    ClipId, ClipSource, CommandId, EpisodeId, PodcastId, SpeakerId, StateRevision, validate_clip,
};
use rusqlite::params;

use crate::StorageError;
use crate::clip_store_codec::encode_source;
use crate::clip_store_read::require_clips_authoritative;
use crate::library_store::command_was_applied;
use crate::library_store_clip_support::{
    finish_clip_command, require_clip, selected_evidence, validate_clip_target,
};

#[allow(clippy::too_many_arguments)]
pub(crate) fn create_clip_in_transaction(
    transaction: &rusqlite::Transaction<'_>,
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
    require_clips_authoritative(transaction)?;
    if let Some(revision) = command_was_applied(transaction, command_id, command_fingerprint)? {
        require_clip(transaction, clip_id)?;
        return Ok(revision);
    }
    validate_clip(
        start_milliseconds,
        end_milliseconds,
        caption,
        frozen_transcript_text,
        source,
    )
    .map_err(|_| StorageError::InvalidClip)?;
    validate_clip_target(transaction, episode_id, podcast_id)?;
    let (source_code, source_wire) = encode_source(source);
    let evidence = selected_evidence(
        transaction,
        episode_id,
        start_milliseconds,
        end_milliseconds,
    )?;
    transaction
        .execute(
            "INSERT INTO pod0_clips(clip_id,clip_revision,episode_id,podcast_id,start_ms,end_ms,\
             created_at_ms,caption,speaker_id,speaker_label,frozen_transcript_text,source_code,\
             source_wire_code,deleted,evidence_generation_id,evidence_transcript_version_id,\
             evidence_content_digest,evidence_span_id,source_import_id,created_command_id) \
             VALUES(?1,1,?2,?3,?4,?5,?6,?7,?8,NULL,?9,?10,?11,0,?12,?13,?14,?15,NULL,?16)",
            params![
                clip_id.into_bytes().as_slice(),
                episode_id.into_bytes().as_slice(),
                podcast_id.into_bytes().as_slice(),
                i64::try_from(start_milliseconds).map_err(|_| StorageError::InvalidClip)?,
                i64::try_from(end_milliseconds).map_err(|_| StorageError::InvalidClip)?,
                observed_at_ms,
                caption,
                speaker_id.map(|value| value.into_bytes().to_vec()),
                frozen_transcript_text,
                source_code,
                source_wire,
                evidence.map(|value| value.generation_id.into_bytes().to_vec()),
                evidence.map(|value| value.transcript_version_id.into_bytes().to_vec()),
                evidence.map(|value| value.transcript_content_digest.into_bytes().to_vec()),
                evidence.map(|value| value.span_id.into_bytes().to_vec()),
                command_id.into_bytes().as_slice()
            ],
        )
        .map_err(|error| StorageError::sqlite("create clip", error))?;
    finish_clip_command(transaction, command_id, command_fingerprint, observed_at_ms)
}
