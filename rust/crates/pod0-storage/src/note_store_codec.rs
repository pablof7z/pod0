use pod0_domain::{
    ContentDigest, EpisodeId, EvidenceGenerationId, EvidenceSpanId, NoteAuthor,
    NoteEvidenceReference, NoteId, NoteKind, NoteRevision, NoteTarget, TranscriptVersionId,
};

use crate::StorageError;

pub(crate) fn note_id(bytes: &[u8]) -> Result<NoteId, StorageError> {
    Ok(NoteId::from_bytes(id(bytes, "note identity")?))
}

pub(crate) fn note_revision(value: i64) -> Result<NoteRevision, StorageError> {
    let value = u64::try_from(value).map_err(|_| corrupt("note revision"))?;
    if value == 0 {
        return Err(corrupt("note revision"));
    }
    Ok(NoteRevision::new(value))
}

pub(crate) const fn encode_kind(value: NoteKind) -> (i64, Option<i64>) {
    match value {
        NoteKind::Free => (1, None),
        NoteKind::Reflection => (2, None),
        NoteKind::SystemEvent => (3, None),
        NoteKind::Unsupported { wire_code } => (255, Some(wire_code as i64)),
    }
}

pub(crate) fn decode_kind(code: i64, wire: Option<i64>) -> Result<NoteKind, StorageError> {
    match (code, wire) {
        (1, None) => Ok(NoteKind::Free),
        (2, None) => Ok(NoteKind::Reflection),
        (3, None) => Ok(NoteKind::SystemEvent),
        (255, Some(wire)) => Ok(NoteKind::Unsupported {
            wire_code: u32::try_from(wire).map_err(|_| corrupt("note kind"))?,
        }),
        _ => Err(corrupt("note kind")),
    }
}

pub(crate) const fn encode_author(value: NoteAuthor) -> (i64, Option<i64>) {
    match value {
        NoteAuthor::User => (1, None),
        NoteAuthor::Agent => (2, None),
        NoteAuthor::Unsupported { wire_code } => (255, Some(wire_code as i64)),
    }
}

pub(crate) fn decode_author(code: i64, wire: Option<i64>) -> Result<NoteAuthor, StorageError> {
    match (code, wire) {
        (1, None) => Ok(NoteAuthor::User),
        (2, None) => Ok(NoteAuthor::Agent),
        (255, Some(wire)) => Ok(NoteAuthor::Unsupported {
            wire_code: u32::try_from(wire).map_err(|_| corrupt("note author"))?,
        }),
        _ => Err(corrupt("note author")),
    }
}

pub(crate) struct EncodedTarget {
    pub code: i64,
    pub wire: Option<i64>,
    pub note_id: Option<Vec<u8>>,
    pub episode_id: Option<Vec<u8>>,
    pub position_ms: Option<i64>,
}

pub(crate) fn encode_target(value: Option<NoteTarget>) -> Result<EncodedTarget, StorageError> {
    Ok(match value {
        None => EncodedTarget {
            code: 0,
            wire: None,
            note_id: None,
            episode_id: None,
            position_ms: None,
        },
        Some(NoteTarget::Note { note_id }) => EncodedTarget {
            code: 1,
            wire: None,
            note_id: Some(note_id.into_bytes().to_vec()),
            episode_id: None,
            position_ms: None,
        },
        Some(NoteTarget::Episode {
            episode_id,
            position_milliseconds,
        }) => EncodedTarget {
            code: 2,
            wire: None,
            note_id: None,
            episode_id: Some(episode_id.into_bytes().to_vec()),
            position_ms: Some(
                i64::try_from(position_milliseconds).map_err(|_| StorageError::InvalidNote)?,
            ),
        },
        Some(NoteTarget::Unsupported { wire_code }) => EncodedTarget {
            code: 255,
            wire: Some(i64::from(wire_code)),
            note_id: None,
            episode_id: None,
            position_ms: None,
        },
    })
}

pub(crate) fn decode_target(
    code: i64,
    wire: Option<i64>,
    note: Option<Vec<u8>>,
    episode: Option<Vec<u8>>,
    position: Option<i64>,
) -> Result<Option<NoteTarget>, StorageError> {
    match (code, wire, note, episode, position) {
        (0, None, None, None, None) => Ok(None),
        (1, None, Some(note), None, None) => Ok(Some(NoteTarget::Note {
            note_id: note_id(&note)?,
        })),
        (2, None, None, Some(episode), Some(position)) => Ok(Some(NoteTarget::Episode {
            episode_id: EpisodeId::from_bytes(id(&episode, "note episode identity")?),
            position_milliseconds: u64::try_from(position).map_err(|_| corrupt("note position"))?,
        })),
        (255, Some(wire), None, None, None) => Ok(Some(NoteTarget::Unsupported {
            wire_code: u32::try_from(wire).map_err(|_| corrupt("note target"))?,
        })),
        _ => Err(corrupt("note target")),
    }
}

pub(crate) fn decode_evidence(
    generation: Option<Vec<u8>>,
    version: Option<Vec<u8>>,
    digest: Option<Vec<u8>>,
    span: Option<Vec<u8>>,
) -> Result<Option<NoteEvidenceReference>, StorageError> {
    match (generation, version, digest, span) {
        (None, None, None, None) => Ok(None),
        (Some(generation), Some(version), Some(digest), Some(span)) => {
            Ok(Some(NoteEvidenceReference {
                generation_id: EvidenceGenerationId::from_bytes(id(
                    &generation,
                    "note evidence generation",
                )?),
                transcript_version_id: TranscriptVersionId::from_bytes(id(
                    &version,
                    "note transcript version",
                )?),
                transcript_content_digest: ContentDigest::from_bytes(
                    digest
                        .try_into()
                        .map_err(|_| corrupt("note evidence digest"))?,
                ),
                span_id: EvidenceSpanId::from_bytes(id(&span, "note evidence span")?),
            }))
        }
        _ => Err(corrupt("note evidence reference")),
    }
}

fn id(bytes: &[u8], detail: &'static str) -> Result<[u8; 16], StorageError> {
    bytes.try_into().map_err(|_| corrupt(detail))
}

fn corrupt(detail: &'static str) -> StorageError {
    StorageError::CorruptSchema { detail }
}
