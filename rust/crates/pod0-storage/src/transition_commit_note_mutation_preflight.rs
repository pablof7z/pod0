use pod0_application::{ActivitySubject, RequestRejectionReason};
use pod0_domain::{
    CommandId, EpisodeId, NoteId, NoteKind, NoteRevision, NoteTarget, StateRevision,
};

use super::NoteWrite;
use crate::StorageError;
use crate::transition_commit::application_support::legacy_library_receipt;
use crate::transition_commit::note_support::{episode_for_target, revisions};

pub(super) type Preflight = (
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
    fingerprint: &str,
    write: &NoteWrite<'_>,
) -> Result<Preflight, StorageError> {
    let (current, committed) = revisions(connection)?;
    let legacy = legacy_library_receipt(
        connection,
        command_id,
        fingerprint,
        "read note command receipt",
    )?;
    let (subject, episodes, rejection) = match write {
        NoteWrite::Update {
            note_id,
            expected,
            text,
            kind,
            target,
        } => preflight_update(connection, *note_id, *expected, text, *kind, *target)?,
        NoteWrite::SetDeleted {
            note_id, expected, ..
        } => preflight_existing(connection, *note_id, *expected)?,
        NoteWrite::Clear { expected } => {
            let rejection =
                (*expected != current).then_some(RequestRejectionReason::RevisionConflict);
            (
                ActivitySubject::Global,
                clear_episode_ids(connection)?,
                rejection,
            )
        }
    };
    Ok((current, committed, legacy, subject, episodes, rejection))
}

fn preflight_update(
    connection: &rusqlite::Connection,
    note_id: NoteId,
    expected: NoteRevision,
    text: &str,
    kind: NoteKind,
    target: Option<NoteTarget>,
) -> Result<
    (
        ActivitySubject,
        Vec<EpisodeId>,
        Option<RequestRejectionReason>,
    ),
    StorageError,
> {
    let subject = ActivitySubject::Note { note_id };
    let (stored, author, old_target) =
        match crate::library_store_note_support::note_mutation_state(connection, note_id) {
            Ok(value) => value,
            Err(StorageError::EntityNotFound) => {
                return Ok((
                    subject,
                    Vec::new(),
                    Some(RequestRejectionReason::MissingSubject),
                ));
            }
            Err(error) => return Err(error),
        };
    let episodes = target_episode_ids(connection, old_target, target)?;
    let rejection = if stored != expected.value {
        Some(RequestRejectionReason::RevisionConflict)
    } else if pod0_domain::validate_new_note(text, kind, author, target).is_err()
        || !crate::library_store_note_support::target_reference_is_valid(
            connection, note_id, target,
        )?
    {
        Some(RequestRejectionReason::Invalid)
    } else {
        None
    };
    Ok((subject, episodes, rejection))
}

fn preflight_existing(
    connection: &rusqlite::Connection,
    note_id: NoteId,
    expected: NoteRevision,
) -> Result<
    (
        ActivitySubject,
        Vec<EpisodeId>,
        Option<RequestRejectionReason>,
    ),
    StorageError,
> {
    let subject = ActivitySubject::Note { note_id };
    match crate::library_store_note_support::note_mutation_state(connection, note_id) {
        Ok((stored, _, target)) => Ok((
            subject,
            episode_for_target(connection, target)?
                .into_iter()
                .collect(),
            (stored != expected.value).then_some(RequestRejectionReason::RevisionConflict),
        )),
        Err(StorageError::EntityNotFound) => Ok((
            subject,
            Vec::new(),
            Some(RequestRejectionReason::MissingSubject),
        )),
        Err(error) => Err(error),
    }
}

fn target_episode_ids(
    connection: &rusqlite::Connection,
    old: Option<NoteTarget>,
    new: Option<NoteTarget>,
) -> Result<Vec<EpisodeId>, StorageError> {
    let mut values = Vec::new();
    for target in [old, new] {
        if let Some(episode_id) = episode_for_target(connection, target)?
            && !values.contains(&episode_id)
        {
            values.push(episode_id);
        }
    }
    Ok(values)
}

fn clear_episode_ids(connection: &rusqlite::Connection) -> Result<Vec<EpisodeId>, StorageError> {
    let mut statement = connection.prepare(
        "SELECT DISTINCT episode_id FROM (SELECT n.episode_id FROM pod0_notes n WHERE n.deleted=0 \
         UNION SELECT target.episode_id FROM pod0_notes n JOIN pod0_notes target ON \
         target.note_id=n.target_note_id WHERE n.deleted=0 UNION SELECT clip.episode_id FROM \
         pod0_notes n JOIN pod0_clips clip ON clip.clip_id=n.target_clip_id WHERE n.deleted=0) \
         WHERE episode_id IS NOT NULL ORDER BY episode_id",
    ).map_err(|error| StorageError::sqlite("prepare cleared note episodes", error))?;
    let rows = statement
        .query_map([], |row| row.get::<_, Vec<u8>>(0))
        .map_err(|error| StorageError::sqlite("read cleared note episodes", error))?;
    let mut values = Vec::new();
    for row in rows {
        let bytes =
            row.map_err(|error| StorageError::sqlite("decode cleared note episode", error))?;
        let bytes: [u8; 16] = bytes.try_into().map_err(|_| StorageError::CorruptSchema {
            detail: "cleared note episode identity is malformed",
        })?;
        values.push(EpisodeId::from_bytes(bytes));
    }
    Ok(values)
}
