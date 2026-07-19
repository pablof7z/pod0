use pod0_domain::{
    ClipId, ClipRevision, ClipSource, CommandId, EpisodeId, PodcastId, SpeakerId, StateRevision,
    validate_clip,
};
use rusqlite::params;

use crate::StorageError;
use crate::clip_store_codec::encode_source;
use crate::clip_store_read::require_clips_authoritative;
use crate::library_store::{LibraryStore, command_was_applied};
use crate::library_store_clip_support::{
    clip_mutation_state, collection_revision, finish_clip_command, require_clip, selected_evidence,
    validate_clip_target,
};

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
        self.write(|transaction| {
            require_clips_authoritative(transaction)?;
            if let Some(revision) =
                command_was_applied(transaction, command_id, command_fingerprint)?
            {
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
                    "INSERT INTO pod0_clips(clip_id,clip_revision,episode_id,podcast_id,start_ms,\
                     end_ms,created_at_ms,caption,speaker_id,speaker_label,frozen_transcript_text,\
                     source_code,source_wire_code,deleted,evidence_generation_id,\
                     evidence_transcript_version_id,evidence_content_digest,evidence_span_id,\
                     source_import_id,created_command_id) \
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
                        command_id.into_bytes().as_slice(),
                    ],
                )
                .map_err(|error| StorageError::sqlite("create clip", error))?;
            finish_clip_command(transaction, command_id, command_fingerprint, observed_at_ms)
        })
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
        self.write(|transaction| {
            require_clips_authoritative(transaction)?;
            if let Some(revision) =
                command_was_applied(transaction, command_id, command_fingerprint)?
            {
                return Ok(revision);
            }
            let (stored_revision, source, old_start, old_end) =
                clip_mutation_state(transaction, clip_id)?;
            if stored_revision != expected_revision.value {
                return Err(StorageError::RevisionConflict);
            }
            validate_clip(
                start_milliseconds,
                end_milliseconds,
                caption,
                frozen_transcript_text,
                source,
            )
            .map_err(|_| StorageError::InvalidClip)?;
            let bounds_changed = old_start != start_milliseconds || old_end != end_milliseconds;
            let evidence = if bounds_changed {
                let episode: Vec<u8> = transaction
                    .query_row(
                        "SELECT episode_id FROM pod0_clips WHERE clip_id=?1",
                        [clip_id.into_bytes().as_slice()],
                        |row| row.get(0),
                    )
                    .map_err(|error| StorageError::sqlite("read clip episode", error))?;
                selected_evidence(
                    transaction,
                    EpisodeId::from_bytes(
                        episode.try_into().map_err(|_| StorageError::CorruptSchema {
                            detail: "clip episode identity is malformed",
                        })?,
                    ),
                    start_milliseconds,
                    end_milliseconds,
                )?
            } else {
                None
            };
            let changed = transaction
                .execute(
                    "UPDATE pod0_clips SET start_ms=?1,end_ms=?2,caption=?3,speaker_id=?4,\
                     speaker_label=CASE WHEN ?4 IS NULL THEN speaker_label ELSE NULL END,\
                     frozen_transcript_text=?5,clip_revision=clip_revision+1,\
                     evidence_generation_id=CASE WHEN ?6 THEN ?7 ELSE evidence_generation_id END,\
                     evidence_transcript_version_id=CASE WHEN ?6 THEN ?8 ELSE evidence_transcript_version_id END,\
                     evidence_content_digest=CASE WHEN ?6 THEN ?9 ELSE evidence_content_digest END,\
                     evidence_span_id=CASE WHEN ?6 THEN ?10 ELSE evidence_span_id END \
                     WHERE clip_id=?11 AND clip_revision=?12",
                    params![
                        i64::try_from(start_milliseconds).map_err(|_| StorageError::InvalidClip)?,
                        i64::try_from(end_milliseconds).map_err(|_| StorageError::InvalidClip)?,
                        caption,
                        speaker_id.map(|value| value.into_bytes().to_vec()),
                        frozen_transcript_text,
                        i64::from(bounds_changed),
                        evidence.map(|value| value.generation_id.into_bytes().to_vec()),
                        evidence.map(|value| value.transcript_version_id.into_bytes().to_vec()),
                        evidence.map(|value| value.transcript_content_digest.into_bytes().to_vec()),
                        evidence.map(|value| value.span_id.into_bytes().to_vec()),
                        clip_id.into_bytes().as_slice(),
                        i64::try_from(expected_revision.value)
                            .map_err(|_| StorageError::RevisionConflict)?,
                    ],
                )
                .map_err(|error| StorageError::sqlite("update clip", error))?;
            if changed != 1 {
                return Err(StorageError::RevisionConflict);
            }
            finish_clip_command(transaction, command_id, command_fingerprint, observed_at_ms)
        })
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
        self.write(|transaction| {
            require_clips_authoritative(transaction)?;
            if let Some(revision) =
                command_was_applied(transaction, command_id, command_fingerprint)?
            {
                return Ok(revision);
            }
            let (stored_revision, _, _, _) = clip_mutation_state(transaction, clip_id)?;
            if stored_revision != expected_revision.value {
                return Err(StorageError::RevisionConflict);
            }
            let changed = transaction
                .execute(
                    "UPDATE pod0_clips SET deleted=?1,clip_revision=clip_revision+1 \
                     WHERE clip_id=?2 AND clip_revision=?3",
                    params![
                        i64::from(deleted),
                        clip_id.into_bytes().as_slice(),
                        i64::try_from(expected_revision.value)
                            .map_err(|_| StorageError::RevisionConflict)?,
                    ],
                )
                .map_err(|error| StorageError::sqlite("update clip deletion", error))?;
            if changed != 1 {
                return Err(StorageError::RevisionConflict);
            }
            finish_clip_command(transaction, command_id, command_fingerprint, observed_at_ms)
        })
    }

    pub fn clear_clips(
        &self,
        command_id: CommandId,
        command_fingerprint: &str,
        expected_collection_revision: StateRevision,
        observed_at_ms: i64,
    ) -> Result<StateRevision, StorageError> {
        self.write(|transaction| {
            require_clips_authoritative(transaction)?;
            if let Some(revision) =
                command_was_applied(transaction, command_id, command_fingerprint)?
            {
                return Ok(revision);
            }
            if collection_revision(transaction)? != expected_collection_revision {
                return Err(StorageError::RevisionConflict);
            }
            transaction
                .execute(
                    "UPDATE pod0_clips SET deleted=1,clip_revision=clip_revision+1 WHERE deleted=0",
                    [],
                )
                .map_err(|error| StorageError::sqlite("clear clips", error))?;
            finish_clip_command(transaction, command_id, command_fingerprint, observed_at_ms)
        })
    }
}
