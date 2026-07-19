use pod0_domain::{
    ClipEvidenceReference, ClipId, ClipSource, CommandId, EpisodeId, PodcastId, StateRevision,
};
use rusqlite::{OptionalExtension, Transaction, params};

use crate::library_store::finish_command;
use crate::{StorageError, clip_store_codec};

pub(crate) fn finish_clip_command(
    transaction: &Transaction<'_>,
    command_id: CommandId,
    fingerprint: &str,
    observed_at_ms: i64,
) -> Result<StateRevision, StorageError> {
    let revision = finish_command(transaction, command_id, fingerprint, observed_at_ms)?;
    set_clip_revision(transaction, revision)?;
    Ok(revision)
}

pub(crate) fn set_clip_revision(
    transaction: &Transaction<'_>,
    revision: StateRevision,
) -> Result<(), StorageError> {
    let value = i64::try_from(revision.value).map_err(|_| StorageError::CorruptSchema {
        detail: "clip collection revision is malformed",
    })?;
    transaction
        .execute(
            "UPDATE pod0_clip_state SET collection_revision=?1 WHERE singleton=1",
            [value],
        )
        .map_err(|error| StorageError::sqlite("advance clip collection revision", error))?;
    transaction
        .execute(
            "UPDATE pod0_domain_cutovers SET core_revision=?1 WHERE domain='clips'",
            [value],
        )
        .map_err(|error| StorageError::sqlite("advance clip cutover revision", error))?;
    Ok(())
}

pub(crate) fn collection_revision(
    transaction: &Transaction<'_>,
) -> Result<StateRevision, StorageError> {
    let value: i64 = transaction
        .query_row(
            "SELECT collection_revision FROM pod0_clip_state WHERE singleton=1",
            [],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("read clip collection revision", error))?;
    Ok(StateRevision::new(u64::try_from(value).map_err(|_| {
        StorageError::CorruptSchema {
            detail: "clip collection revision is malformed",
        }
    })?))
}

pub(crate) fn require_clip(
    transaction: &Transaction<'_>,
    clip_id: ClipId,
) -> Result<(), StorageError> {
    let exists: bool = transaction
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM pod0_clips WHERE clip_id=?1)",
            [clip_id.into_bytes().as_slice()],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("find clip", error))?;
    if exists {
        Ok(())
    } else {
        Err(StorageError::EntityNotFound)
    }
}

pub(crate) fn clip_mutation_state(
    transaction: &Transaction<'_>,
    clip_id: ClipId,
) -> Result<(u64, ClipSource, u64, u64), StorageError> {
    let row = transaction
        .query_row(
            "SELECT clip_revision,source_code,source_wire_code,start_ms,end_ms \
             FROM pod0_clips WHERE clip_id=?1",
            [clip_id.into_bytes().as_slice()],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, Option<i64>>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                ))
            },
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read clip mutation state", error))?
        .ok_or(StorageError::EntityNotFound)?;
    Ok((
        u64::try_from(row.0).map_err(|_| StorageError::CorruptSchema {
            detail: "clip revision is malformed",
        })?,
        clip_store_codec::decode_source(row.1, row.2)?,
        u64::try_from(row.3).map_err(|_| StorageError::CorruptSchema {
            detail: "clip start is malformed",
        })?,
        u64::try_from(row.4).map_err(|_| StorageError::CorruptSchema {
            detail: "clip end is malformed",
        })?,
    ))
}

pub(crate) fn validate_clip_target(
    transaction: &Transaction<'_>,
    episode_id: EpisodeId,
    podcast_id: PodcastId,
) -> Result<(), StorageError> {
    let stored: Option<Vec<u8>> = transaction
        .query_row(
            "SELECT podcast_id FROM pod0_episodes WHERE episode_id=?1",
            [episode_id.into_bytes().as_slice()],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| StorageError::sqlite("validate clip target", error))?;
    if stored.as_deref() == Some(podcast_id.into_bytes().as_slice()) {
        Ok(())
    } else {
        Err(StorageError::InvalidClip)
    }
}

pub(crate) fn selected_evidence(
    transaction: &Transaction<'_>,
    episode_id: EpisodeId,
    start_milliseconds: u64,
    end_milliseconds: u64,
) -> Result<Option<ClipEvidenceReference>, StorageError> {
    let start = i64::try_from(start_milliseconds).map_err(|_| StorageError::InvalidClip)?;
    let end = i64::try_from(end_milliseconds).map_err(|_| StorageError::InvalidClip)?;
    let row = transaction
        .query_row(
            "SELECT s.generation_id,g.transcript_version_id,d.content_digest,s.span_id \
             FROM pod0_evidence_selection selected \
             JOIN pod0_evidence_generations g ON g.generation_id=selected.generation_id \
             JOIN pod0_transcript_documents d ON d.transcript_version_id=g.transcript_version_id \
             JOIN pod0_evidence_spans s ON s.generation_id=g.generation_id \
             WHERE selected.episode_id=?1 AND g.state='verified' \
             AND s.start_ms<=?2 AND s.end_ms>=?3 ORDER BY s.sort_order LIMIT 1",
            params![episode_id.into_bytes().as_slice(), start, end],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()
        .map_err(|error| StorageError::sqlite("select clip evidence", error))?;
    let Some((generation, version, digest, span)) = row else {
        return Ok(None);
    };
    clip_store_codec::decode_evidence(Some(generation), Some(version), Some(digest), Some(span))
}
