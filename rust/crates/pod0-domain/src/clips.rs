use crate::{
    ContentDigest, EpisodeId, EvidenceGenerationId, EvidenceSpanId, PodcastId, SpeakerId,
    TranscriptVersionId, UnixTimestampMilliseconds,
};

pub const MAX_CLIP_CAPTION_BYTES: usize = 4_096;
pub const MAX_CLIP_TEXT_BYTES: usize = 65_536;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, uniffi::Record)]
pub struct ClipRevision {
    pub value: u64,
}

impl ClipRevision {
    pub const INITIAL: Self = Self { value: 1 };

    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self { value }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum ClipSource {
    Touch,
    Auto,
    Headphone,
    Carplay,
    Watch,
    Siri,
    Agent,
    Unsupported { wire_code: u32 },
}

/// Immutable provenance captured from the selected, verified evidence
/// generation. Transcript rebuilds never silently retarget a saved clip.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ClipEvidenceReference {
    pub generation_id: EvidenceGenerationId,
    pub transcript_version_id: TranscriptVersionId,
    pub transcript_content_digest: ContentDigest,
    pub span_id: EvidenceSpanId,
}

/// Durable clip state owned by the Pod0 kernel. Media rendering and sharing
/// remain native capabilities over this bounded projection.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ClipRecord {
    pub clip_id: crate::ClipId,
    pub revision: ClipRevision,
    pub episode_id: EpisodeId,
    pub podcast_id: PodcastId,
    pub start_milliseconds: u64,
    pub end_milliseconds: u64,
    pub created_at: UnixTimestampMilliseconds,
    pub caption: Option<String>,
    pub speaker_id: Option<SpeakerId>,
    /// Preserved only for pre-kernel clips whose Swift payload used a display
    /// label instead of a stable speaker identity.
    pub speaker_label: Option<String>,
    pub frozen_transcript_text: String,
    pub source: ClipSource,
    pub deleted: bool,
    pub evidence: Option<ClipEvidenceReference>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipValidationError {
    InvalidBounds,
    CaptionTooLarge,
    TextTooLarge,
    UnsupportedSource,
    InvalidRevision,
}

pub fn validate_clip(
    start_milliseconds: u64,
    end_milliseconds: u64,
    caption: Option<&str>,
    frozen_transcript_text: &str,
    source: ClipSource,
) -> Result<(), ClipValidationError> {
    if start_milliseconds >= end_milliseconds || end_milliseconds > i64::MAX as u64 {
        return Err(ClipValidationError::InvalidBounds);
    }
    if caption.is_some_and(|value| value.len() > MAX_CLIP_CAPTION_BYTES) {
        return Err(ClipValidationError::CaptionTooLarge);
    }
    if frozen_transcript_text.len() > MAX_CLIP_TEXT_BYTES {
        return Err(ClipValidationError::TextTooLarge);
    }
    if matches!(source, ClipSource::Unsupported { .. }) {
        return Err(ClipValidationError::UnsupportedSource);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clips_reject_invalid_bounds_and_unbounded_values() {
        assert_eq!(
            validate_clip(20, 20, None, "", ClipSource::Touch),
            Err(ClipValidationError::InvalidBounds)
        );
        assert_eq!(
            validate_clip(
                10,
                20,
                Some(&"x".repeat(MAX_CLIP_CAPTION_BYTES + 1)),
                "",
                ClipSource::Touch,
            ),
            Err(ClipValidationError::CaptionTooLarge)
        );
        assert!(validate_clip(10, 20, Some("Moment"), "Exact words", ClipSource::Agent).is_ok());
    }
}
