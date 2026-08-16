use pod0_application::{
    ActivityActor, ActivityOrigin, ActivitySubject, RequestDisposition, RequestRejectionReason,
    UserArtifactTransition,
};
use pod0_domain::{EpisodeId, SpeakerEntityId, SpeakerId, TranscriptArtifactId};
use rusqlite::{OptionalExtension, params};

use super::SpeakerWrite;
use crate::StorageError;
use crate::speaker_store_model::{
    SpeakerAssignmentOrigin, decode_assignment_origin, validate_confidence, validate_display_name,
};

pub(super) struct Preflight {
    pub(super) actor: ActivityActor,
    pub(super) origin: ActivityOrigin,
    pub(super) subject: ActivitySubject,
    pub(super) episode_id: Option<EpisodeId>,
    pub(super) transition: UserArtifactTransition,
    pub(super) disposition: RequestDisposition,
}

pub(super) fn preflight(
    transaction: &rusqlite::Transaction<'_>,
    write: &SpeakerWrite<'_>,
    observed_at_ms: i64,
) -> Result<Preflight, StorageError> {
    match write {
        SpeakerWrite::Create {
            entity_id,
            display_name,
        } => identity_preflight(transaction, *entity_id, display_name, None, observed_at_ms),
        SpeakerWrite::Rename {
            entity_id,
            expected_entity_revision,
            display_name,
        } => identity_preflight(
            transaction,
            *entity_id,
            display_name,
            Some(*expected_entity_revision),
            observed_at_ms,
        ),
        SpeakerWrite::Assign {
            artifact_id,
            speaker_id,
            entity_id,
            origin,
            confidence,
        } => assignment_preflight(
            transaction,
            *artifact_id,
            *speaker_id,
            *entity_id,
            *origin,
            *confidence,
            observed_at_ms,
        ),
    }
}

fn identity_preflight(
    transaction: &rusqlite::Transaction<'_>,
    entity_id: SpeakerEntityId,
    display_name: &str,
    expected_revision: Option<u64>,
    observed_at_ms: i64,
) -> Result<Preflight, StorageError> {
    let existing = transaction
        .query_row(
            "SELECT speaker_entity_revision,display_name,deleted FROM pod0_speakers \
             WHERE speaker_entity_id=?1",
            [entity_id.into_bytes().as_slice()],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            },
        )
        .optional()
        .map_err(|error| StorageError::sqlite("preflight speaker identity", error))?;
    let invalid = validate_display_name(display_name).is_err() || observed_at_ms < 0;
    let disposition = match (expected_revision, existing) {
        (_, _) if invalid => rejected(RequestRejectionReason::Invalid),
        (None, Some(_)) => rejected(RequestRejectionReason::RevisionConflict),
        (None, None) => RequestDisposition::Accepted,
        (Some(_), None) => rejected(RequestRejectionReason::MissingSubject),
        (Some(_), Some((_, _, deleted))) if deleted != 0 => {
            rejected(RequestRejectionReason::MissingSubject)
        }
        (Some(expected), Some((actual, _, _))) if u64::try_from(actual).ok() != Some(expected) => {
            rejected(RequestRejectionReason::RevisionConflict)
        }
        (Some(_), Some((_, stored, _))) if stored == display_name => {
            RequestDisposition::NoSemanticChange
        }
        (Some(_), Some(_)) => RequestDisposition::Accepted,
    };
    Ok(Preflight {
        actor: ActivityActor::User,
        origin: ActivityOrigin::UserInterface,
        subject: ActivitySubject::SpeakerEntity {
            speaker_entity_id: entity_id,
        },
        episode_id: None,
        transition: UserArtifactTransition::SpeakerIdentityChanged,
        disposition,
    })
}

#[allow(clippy::too_many_arguments)]
fn assignment_preflight(
    transaction: &rusqlite::Transaction<'_>,
    artifact_id: TranscriptArtifactId,
    speaker_id: SpeakerId,
    entity_id: SpeakerEntityId,
    origin: SpeakerAssignmentOrigin,
    confidence: Option<f64>,
    observed_at_ms: i64,
) -> Result<Preflight, StorageError> {
    let episode_id = artifact_episode(transaction, artifact_id)?;
    let entity_exists =
        match crate::speaker_store_write::require_active_entity(transaction, entity_id) {
            Ok(()) => true,
            Err(StorageError::EntityNotFound) => false,
            Err(error) => return Err(error),
        };
    let speaker_exists = match crate::speaker_store_write::require_artifact_speaker(
        transaction,
        artifact_id,
        speaker_id,
    ) {
        Ok(()) => true,
        Err(StorageError::TranscriptNotFound) => false,
        Err(error) => return Err(error),
    };
    let existing = existing_assignment(transaction, artifact_id, speaker_id)?;
    let invalid = validate_confidence(confidence).is_err() || observed_at_ms < 0;
    let blocked = existing.as_ref().is_some_and(|(_, stored_origin, _)| {
        *stored_origin == SpeakerAssignmentOrigin::User && origin != SpeakerAssignmentOrigin::User
    });
    let unchanged = existing
        .as_ref()
        .is_some_and(|(stored_entity, stored_origin, stored)| {
            *stored_entity == entity_id && *stored_origin == origin && *stored == confidence
        });
    let disposition = if invalid {
        rejected(RequestRejectionReason::Invalid)
    } else if episode_id.is_none() || !entity_exists || !speaker_exists {
        rejected(RequestRejectionReason::MissingSubject)
    } else if blocked {
        rejected(RequestRejectionReason::NotAllowed)
    } else if unchanged {
        RequestDisposition::NoSemanticChange
    } else {
        RequestDisposition::Accepted
    };
    let (actor, activity_origin) = match origin {
        SpeakerAssignmentOrigin::User => (ActivityActor::User, ActivityOrigin::UserInterface),
        SpeakerAssignmentOrigin::Inferred | SpeakerAssignmentOrigin::FeedMetadata => {
            (ActivityActor::System, ActivityOrigin::AutomaticPolicy)
        }
    };
    Ok(Preflight {
        actor,
        origin: activity_origin,
        subject: ActivitySubject::TranscriptArtifact { artifact_id },
        episode_id,
        transition: UserArtifactTransition::SpeakerAssignmentChanged,
        disposition,
    })
}

fn existing_assignment(
    transaction: &rusqlite::Transaction<'_>,
    artifact_id: TranscriptArtifactId,
    speaker_id: SpeakerId,
) -> Result<Option<(SpeakerEntityId, SpeakerAssignmentOrigin, Option<f64>)>, StorageError> {
    transaction
        .query_row(
            "SELECT speaker_entity_id,origin_code,confidence FROM pod0_speaker_assignments \
             WHERE artifact_id=?1 AND speaker_id=?2",
            params![
                artifact_id.into_bytes().as_slice(),
                speaker_id.into_bytes().as_slice()
            ],
            |row| {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, Option<f64>>(2)?,
                ))
            },
        )
        .optional()
        .map_err(|error| StorageError::sqlite("preflight speaker assignment", error))?
        .map(|(entity, origin, confidence)| {
            let bytes: [u8; 16] = entity
                .try_into()
                .map_err(|_| StorageError::InvalidSpeakerEntity)?;
            Ok((
                SpeakerEntityId::from_bytes(bytes),
                decode_assignment_origin(origin)?,
                confidence,
            ))
        })
        .transpose()
}

fn artifact_episode(
    transaction: &rusqlite::Transaction<'_>,
    artifact_id: TranscriptArtifactId,
) -> Result<Option<EpisodeId>, StorageError> {
    transaction
        .query_row(
            "SELECT episode_id FROM pod0_transcript_artifacts WHERE artifact_id=?1",
            [artifact_id.into_bytes().as_slice()],
            |row| row.get::<_, Vec<u8>>(0),
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read speaker artifact episode", error))?
        .map(|value| {
            value
                .try_into()
                .map(EpisodeId::from_bytes)
                .map_err(|_| StorageError::InvalidTranscriptArtifact)
        })
        .transpose()
}

const fn rejected(reason: RequestRejectionReason) -> RequestDisposition {
    RequestDisposition::Rejected { reason }
}
