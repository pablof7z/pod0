use pod0_domain::{ClipId, ClipRevision, EpisodeId, SpeakerId, StateRevision, validate_clip};
use rusqlite::params;

use crate::StorageError;
use crate::clip_store_read::require_clips_authoritative;
use crate::library_store::command_was_applied;
use crate::library_store_clip_support::{
    clip_mutation_state, collection_revision, finish_clip_command, selected_evidence,
};

#[allow(clippy::too_many_arguments)]
pub(crate) fn update_clip_in_transaction(
    transaction: &rusqlite::Transaction<'_>,
    command_id: pod0_domain::CommandId,
    fingerprint: &str,
    clip_id: ClipId,
    expected: ClipRevision,
    start: u64,
    end: u64,
    caption: Option<&str>,
    speaker_id: Option<SpeakerId>,
    frozen_text: &str,
    observed_at_ms: i64,
) -> Result<StateRevision, StorageError> {
    require_clips_authoritative(transaction)?;
    if let Some(revision) = command_was_applied(transaction, command_id, fingerprint)? {
        return Ok(revision);
    }
    let (stored, source, old_start, old_end) = clip_mutation_state(transaction, clip_id)?;
    if stored != expected.value {
        return Err(StorageError::RevisionConflict);
    }
    validate_clip(start, end, caption, frozen_text, source)
        .map_err(|_| StorageError::InvalidClip)?;
    let bounds_changed = old_start != start || old_end != end;
    let evidence = if bounds_changed {
        selected_evidence(transaction, clip_episode(transaction, clip_id)?, start, end)?
    } else {
        None
    };
    let changed = transaction.execute(
        "UPDATE pod0_clips SET start_ms=?1,end_ms=?2,caption=?3,speaker_id=?4,\
         speaker_label=CASE WHEN ?4 IS NULL THEN speaker_label ELSE NULL END,\
         frozen_transcript_text=?5,clip_revision=clip_revision+1,\
         evidence_generation_id=CASE WHEN ?6 THEN ?7 ELSE evidence_generation_id END,\
         evidence_transcript_version_id=CASE WHEN ?6 THEN ?8 ELSE evidence_transcript_version_id END,\
         evidence_content_digest=CASE WHEN ?6 THEN ?9 ELSE evidence_content_digest END,\
         evidence_span_id=CASE WHEN ?6 THEN ?10 ELSE evidence_span_id END \
         WHERE clip_id=?11 AND clip_revision=?12",
        params![i64::try_from(start).map_err(|_| StorageError::InvalidClip)?,
            i64::try_from(end).map_err(|_| StorageError::InvalidClip)?, caption,
            speaker_id.map(|value| value.into_bytes().to_vec()), frozen_text,
            i64::from(bounds_changed),
            evidence.map(|value| value.generation_id.into_bytes().to_vec()),
            evidence.map(|value| value.transcript_version_id.into_bytes().to_vec()),
            evidence.map(|value| value.transcript_content_digest.into_bytes().to_vec()),
            evidence.map(|value| value.span_id.into_bytes().to_vec()),
            clip_id.into_bytes().as_slice(),
            i64::try_from(expected.value).map_err(|_| StorageError::RevisionConflict)?],
    ).map_err(|error| StorageError::sqlite("update clip", error))?;
    finish(
        transaction,
        command_id,
        fingerprint,
        changed,
        observed_at_ms,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn set_clip_deleted_in_transaction(
    transaction: &rusqlite::Transaction<'_>,
    command_id: pod0_domain::CommandId,
    fingerprint: &str,
    clip_id: ClipId,
    expected: ClipRevision,
    deleted: bool,
    observed_at_ms: i64,
) -> Result<StateRevision, StorageError> {
    require_clips_authoritative(transaction)?;
    if let Some(revision) = command_was_applied(transaction, command_id, fingerprint)? {
        return Ok(revision);
    }
    if clip_mutation_state(transaction, clip_id)?.0 != expected.value {
        return Err(StorageError::RevisionConflict);
    }
    let changed = transaction
        .execute(
            "UPDATE pod0_clips SET deleted=?1,clip_revision=clip_revision+1 \
         WHERE clip_id=?2 AND clip_revision=?3",
            params![
                i64::from(deleted),
                clip_id.into_bytes().as_slice(),
                i64::try_from(expected.value).map_err(|_| StorageError::RevisionConflict)?
            ],
        )
        .map_err(|error| StorageError::sqlite("update clip deletion", error))?;
    finish(
        transaction,
        command_id,
        fingerprint,
        changed,
        observed_at_ms,
    )
}

pub(crate) fn clear_clips_in_transaction(
    transaction: &rusqlite::Transaction<'_>,
    command_id: pod0_domain::CommandId,
    fingerprint: &str,
    expected: StateRevision,
    observed_at_ms: i64,
) -> Result<StateRevision, StorageError> {
    require_clips_authoritative(transaction)?;
    if let Some(revision) = command_was_applied(transaction, command_id, fingerprint)? {
        return Ok(revision);
    }
    if collection_revision(transaction)? != expected {
        return Err(StorageError::RevisionConflict);
    }
    transaction
        .execute(
            "UPDATE pod0_clips SET deleted=1,clip_revision=clip_revision+1 WHERE deleted=0",
            [],
        )
        .map_err(|error| StorageError::sqlite("clear clips", error))?;
    finish_clip_command(transaction, command_id, fingerprint, observed_at_ms)
}

fn finish(
    transaction: &rusqlite::Transaction<'_>,
    command_id: pod0_domain::CommandId,
    fingerprint: &str,
    changed: usize,
    observed_at_ms: i64,
) -> Result<StateRevision, StorageError> {
    if changed != 1 {
        return Err(StorageError::RevisionConflict);
    }
    finish_clip_command(transaction, command_id, fingerprint, observed_at_ms)
}

pub(crate) fn clip_episode(
    connection: &rusqlite::Connection,
    clip_id: ClipId,
) -> Result<EpisodeId, StorageError> {
    let bytes: Vec<u8> = connection
        .query_row(
            "SELECT episode_id FROM pod0_clips WHERE clip_id=?1",
            [clip_id.into_bytes().as_slice()],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("read clip episode", error))?;
    Ok(EpisodeId::from_bytes(bytes.try_into().map_err(|_| {
        StorageError::CorruptSchema {
            detail: "clip episode identity is malformed",
        }
    })?))
}
