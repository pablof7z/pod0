use pod0_application::{ActivitySubject, RequestRejectionReason};
use pod0_domain::{ClipId, ClipRevision, CommandId, EpisodeId, StateRevision};

use super::ClipWrite;
use crate::StorageError;
use crate::transition_commit::application_support::{legacy_library_receipt, next_core_revision};

type Preflight = (
    StateRevision,
    StateRevision,
    Option<StateRevision>,
    ActivitySubject,
    Vec<EpisodeId>,
    Option<RequestRejectionReason>,
);

pub(super) fn preflight(
    connection: &rusqlite::Connection,
    command_id: CommandId,
    command_fingerprint: &str,
    write: &ClipWrite<'_>,
) -> Result<Preflight, StorageError> {
    crate::clip_store_read::require_clips_authoritative(connection)?;
    let current = crate::library_store_clip_support::collection_revision(connection)?;
    let committed = next_core_revision(connection, "read clip core revision")?;
    let legacy = legacy_library_receipt(
        connection,
        command_id,
        command_fingerprint,
        "read clip command receipt",
    )?;
    let (subject, episodes, rejection) = match write {
        ClipWrite::Update {
            clip_id,
            expected,
            start,
            end,
            caption,
            frozen_text,
            ..
        } => preflight_update(
            connection,
            *clip_id,
            *expected,
            *start,
            *end,
            *caption,
            frozen_text,
        )?,
        ClipWrite::SetDeleted {
            clip_id, expected, ..
        } => existing(connection, *clip_id, *expected)?,
        ClipWrite::Clear { expected } => (
            ActivitySubject::Global,
            clear_episode_ids(connection)?,
            (*expected != current).then_some(RequestRejectionReason::RevisionConflict),
        ),
    };
    Ok((current, committed, legacy, subject, episodes, rejection))
}

#[allow(clippy::too_many_arguments)]
fn preflight_update(
    connection: &rusqlite::Connection,
    clip_id: ClipId,
    expected: ClipRevision,
    start: u64,
    end: u64,
    caption: Option<&str>,
    frozen_text: &str,
) -> Result<
    (
        ActivitySubject,
        Vec<EpisodeId>,
        Option<RequestRejectionReason>,
    ),
    StorageError,
> {
    let subject = ActivitySubject::Clip { clip_id };
    match crate::library_store_clip_support::clip_mutation_state(connection, clip_id) {
        Ok((revision, source, _, _)) => {
            let episode = crate::library_store_clip_mutation::clip_episode(connection, clip_id)?;
            let rejection = if revision != expected.value {
                Some(RequestRejectionReason::RevisionConflict)
            } else if pod0_domain::validate_clip(start, end, caption, frozen_text, source).is_err()
            {
                Some(RequestRejectionReason::Invalid)
            } else {
                None
            };
            Ok((subject, vec![episode], rejection))
        }
        Err(StorageError::EntityNotFound) => Ok((
            subject,
            Vec::new(),
            Some(RequestRejectionReason::MissingSubject),
        )),
        Err(error) => Err(error),
    }
}

fn existing(
    connection: &rusqlite::Connection,
    clip_id: ClipId,
    expected: ClipRevision,
) -> Result<
    (
        ActivitySubject,
        Vec<EpisodeId>,
        Option<RequestRejectionReason>,
    ),
    StorageError,
> {
    let subject = ActivitySubject::Clip { clip_id };
    match crate::library_store_clip_support::clip_mutation_state(connection, clip_id) {
        Ok((revision, _, _, _)) => Ok((
            subject,
            vec![crate::library_store_clip_mutation::clip_episode(
                connection, clip_id,
            )?],
            (revision != expected.value).then_some(RequestRejectionReason::RevisionConflict),
        )),
        Err(StorageError::EntityNotFound) => Ok((
            subject,
            Vec::new(),
            Some(RequestRejectionReason::MissingSubject),
        )),
        Err(error) => Err(error),
    }
}

fn clear_episode_ids(connection: &rusqlite::Connection) -> Result<Vec<EpisodeId>, StorageError> {
    let mut statement = connection
        .prepare("SELECT DISTINCT episode_id FROM pod0_clips WHERE deleted=0 ORDER BY episode_id")
        .map_err(|error| StorageError::sqlite("prepare cleared clip episodes", error))?;
    let rows = statement
        .query_map([], |row| row.get::<_, Vec<u8>>(0))
        .map_err(|error| StorageError::sqlite("read cleared clip episodes", error))?;
    rows.map(|row| {
        let bytes =
            row.map_err(|error| StorageError::sqlite("decode cleared clip episode", error))?;
        Ok(EpisodeId::from_bytes(bytes.try_into().map_err(|_| {
            StorageError::CorruptSchema {
                detail: "cleared clip episode identity is malformed",
            }
        })?))
    })
    .collect()
}
