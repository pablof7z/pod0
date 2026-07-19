use crate::{
    ContentDigest, EpisodeId, EvidenceGenerationId, EvidenceSpanId, NoteId, TranscriptVersionId,
    UnixTimestampMilliseconds,
};

pub const MAX_NOTE_TEXT_BYTES: usize = 65_536;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, uniffi::Record)]
pub struct NoteRevision {
    pub value: u64,
}

impl NoteRevision {
    pub const INITIAL: Self = Self { value: 1 };

    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self { value }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum NoteKind {
    Free,
    Reflection,
    SystemEvent,
    Unsupported { wire_code: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum NoteAuthor {
    User,
    Agent,
    Unsupported { wire_code: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum NoteTarget {
    Note {
        note_id: NoteId,
    },
    Episode {
        episode_id: EpisodeId,
        position_milliseconds: u64,
    },
    Unsupported {
        wire_code: u32,
    },
}

/// Immutable provenance captured from the selected, verified evidence
/// generation at note creation. Later transcript rebuilds never retarget it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Record)]
pub struct NoteEvidenceReference {
    pub generation_id: EvidenceGenerationId,
    pub transcript_version_id: TranscriptVersionId,
    pub transcript_content_digest: ContentDigest,
    pub span_id: EvidenceSpanId,
}

/// Durable note state owned by the Pod0 kernel. Native shells may map this to
/// presentation values but must not persist or mutate an independent copy.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct NoteRecord {
    pub note_id: NoteId,
    pub revision: NoteRevision,
    pub text: String,
    pub kind: NoteKind,
    pub author: NoteAuthor,
    pub target: Option<NoteTarget>,
    pub created_at: UnixTimestampMilliseconds,
    pub deleted: bool,
    pub evidence: Option<NoteEvidenceReference>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoteValidationError {
    EmptyText,
    TextTooLarge,
    UnsupportedKind,
    UnsupportedAuthor,
    UnsupportedTarget,
    InvalidRevision,
}

pub fn validate_new_note(
    text: &str,
    kind: NoteKind,
    author: NoteAuthor,
    target: Option<NoteTarget>,
) -> Result<(), NoteValidationError> {
    if text.trim().is_empty() {
        return Err(NoteValidationError::EmptyText);
    }
    if text.len() > MAX_NOTE_TEXT_BYTES {
        return Err(NoteValidationError::TextTooLarge);
    }
    if matches!(kind, NoteKind::Unsupported { .. }) {
        return Err(NoteValidationError::UnsupportedKind);
    }
    if matches!(author, NoteAuthor::Unsupported { .. }) {
        return Err(NoteValidationError::UnsupportedAuthor);
    }
    if matches!(target, Some(NoteTarget::Unsupported { .. })) {
        return Err(NoteValidationError::UnsupportedTarget);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_notes_reject_ambiguous_or_unbounded_values() {
        assert_eq!(
            validate_new_note("  ", NoteKind::Free, NoteAuthor::User, None),
            Err(NoteValidationError::EmptyText)
        );
        assert_eq!(
            validate_new_note(
                &"x".repeat(MAX_NOTE_TEXT_BYTES + 1),
                NoteKind::Free,
                NoteAuthor::User,
                None,
            ),
            Err(NoteValidationError::TextTooLarge)
        );
        assert!(
            validate_new_note(
                "Remember this",
                NoteKind::Reflection,
                NoteAuthor::Agent,
                Some(NoteTarget::Episode {
                    episode_id: EpisodeId::from_parts(1, 2),
                    position_milliseconds: 42_125,
                }),
            )
            .is_ok()
        );
    }
}
