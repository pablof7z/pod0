use crate::transcript_artifact_hash::transcript_artifact_digest;
use crate::transcript_artifact_validation::{canonical_segments, validate_metadata};
use crate::{
    ContentDigest, EpisodeId, PodcastId, SpeakerId, TranscriptArtifactId, TranscriptProvenance,
    TranscriptSegmentId, TranscriptSource, TranscriptVersionId, UnixTimestampMilliseconds,
    transcript_content_digest, transcript_segment_id, transcript_version_id,
};

pub const TRANSCRIPT_ARTIFACT_SCHEMA_VERSION: u32 = 1;
pub const MAX_TRANSCRIPT_LANGUAGE_BYTES: usize = 64;
pub const MAX_TRANSCRIPT_SPEAKERS: usize = 4_096;
pub const MAX_TRANSCRIPT_WORDS: usize = 2_000_000;
pub const MAX_TRANSCRIPT_SPEAKER_LABEL_BYTES: usize = 1_024;
pub const MAX_TRANSCRIPT_WORD_TEXT_BYTES: usize = 1_024;

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct TranscriptArtifactSpeakerInput {
    pub speaker_id: SpeakerId,
    pub label: String,
    pub display_name: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct TranscriptArtifactWordInput {
    pub text: String,
    pub start_milliseconds: u64,
    pub end_milliseconds: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct TranscriptArtifactSegmentInput {
    pub text: String,
    pub start_milliseconds: u64,
    pub end_milliseconds: u64,
    pub speaker_id: Option<SpeakerId>,
    pub words: Vec<TranscriptArtifactWordInput>,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct TranscriptArtifactInput {
    pub episode_id: EpisodeId,
    pub podcast_id: PodcastId,
    pub source_revision: String,
    pub source: TranscriptSource,
    pub provider: Option<String>,
    pub source_payload_digest: ContentDigest,
    pub language: String,
    pub generated_at: UnixTimestampMilliseconds,
    pub speakers: Vec<TranscriptArtifactSpeakerInput>,
    pub segments: Vec<TranscriptArtifactSegmentInput>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TranscriptArtifactSegment {
    pub segment_id: TranscriptSegmentId,
    pub ordinal: u32,
    pub text: String,
    pub start_milliseconds: u64,
    pub end_milliseconds: u64,
    pub speaker_id: Option<SpeakerId>,
    pub words: Vec<TranscriptArtifactWordInput>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TranscriptArtifact {
    pub schema_version: u32,
    pub artifact_id: TranscriptArtifactId,
    pub transcript_version_id: TranscriptVersionId,
    pub content_digest: ContentDigest,
    pub integrity_digest: ContentDigest,
    pub episode_id: EpisodeId,
    pub podcast_id: PodcastId,
    pub source_revision: String,
    pub provenance: TranscriptProvenance,
    pub language: String,
    pub generated_at: UnixTimestampMilliseconds,
    pub speakers: Vec<TranscriptArtifactSpeakerInput>,
    pub segments: Vec<TranscriptArtifactSegment>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TranscriptArtifactError {
    InvalidSourceRevision,
    InvalidProvider,
    InvalidLanguage,
    TooManySpeakers,
    DuplicateSpeaker,
    InvalidSpeakerLabel,
    TooManySegments,
    InvalidSegmentText,
    SegmentTextTooLong,
    InvalidSegmentTime,
    SegmentsOutOfOrder,
    TooManyWords,
    InvalidWordText,
    InvalidWordTime,
    WordsOutOfOrder,
    TranscriptTooLarge,
    IdentityMismatch,
}

impl std::fmt::Display for TranscriptArtifactError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "invalid transcript artifact: {self:?}")
    }
}

impl std::error::Error for TranscriptArtifactError {}

impl TranscriptArtifact {
    pub fn seal(input: TranscriptArtifactInput) -> Result<Self, TranscriptArtifactError> {
        validate_metadata(&input)?;
        let canonical = canonical_segments(&input)?;
        let content_digest = transcript_content_digest(&canonical);
        let provenance = TranscriptProvenance {
            source: input.source,
            provider: input.provider,
            source_payload_digest: input.source_payload_digest,
        };
        let version_id = transcript_version_id(
            input.episode_id,
            input.podcast_id,
            &input.source_revision,
            content_digest,
            &provenance,
        );
        let segments = input
            .segments
            .into_iter()
            .zip(canonical.iter())
            .map(|(source, canonical)| TranscriptArtifactSegment {
                segment_id: transcript_segment_id(version_id, canonical),
                ordinal: canonical.ordinal,
                text: source.text,
                start_milliseconds: source.start_milliseconds,
                end_milliseconds: source.end_milliseconds,
                speaker_id: source.speaker_id,
                words: source.words,
            })
            .collect();
        let mut artifact = Self {
            schema_version: TRANSCRIPT_ARTIFACT_SCHEMA_VERSION,
            artifact_id: TranscriptArtifactId::from_parts(0, 0),
            transcript_version_id: version_id,
            content_digest,
            integrity_digest: ContentDigest::default(),
            episode_id: input.episode_id,
            podcast_id: input.podcast_id,
            source_revision: input.source_revision,
            provenance,
            language: input.language,
            generated_at: input.generated_at,
            speakers: input.speakers,
            segments,
        };
        artifact.integrity_digest = transcript_artifact_digest(&artifact);
        artifact.artifact_id =
            TranscriptArtifactId::from_bytes(first_16(artifact.integrity_digest.into_bytes()));
        artifact.verify_integrity()?;
        Ok(artifact)
    }

    pub fn verify_integrity(&self) -> Result<(), TranscriptArtifactError> {
        let input = self.as_input();
        validate_metadata(&input)?;
        let canonical = canonical_segments(&input)?;
        let expected_content = transcript_content_digest(&canonical);
        let expected_version = transcript_version_id(
            self.episode_id,
            self.podcast_id,
            &self.source_revision,
            expected_content,
            &self.provenance,
        );
        let expected_digest = transcript_artifact_digest(self);
        let segment_ids_match =
            self.segments
                .iter()
                .zip(canonical.iter())
                .all(|(record, content)| {
                    record.segment_id == transcript_segment_id(expected_version, content)
                });
        if self.schema_version != TRANSCRIPT_ARTIFACT_SCHEMA_VERSION
            || self.content_digest != expected_content
            || self.transcript_version_id != expected_version
            || self.integrity_digest != expected_digest
            || self.artifact_id
                != TranscriptArtifactId::from_bytes(first_16(expected_digest.into_bytes()))
            || !segment_ids_match
        {
            return Err(TranscriptArtifactError::IdentityMismatch);
        }
        Ok(())
    }

    fn as_input(&self) -> TranscriptArtifactInput {
        TranscriptArtifactInput {
            episode_id: self.episode_id,
            podcast_id: self.podcast_id,
            source_revision: self.source_revision.clone(),
            source: self.provenance.source,
            provider: self.provenance.provider.clone(),
            source_payload_digest: self.provenance.source_payload_digest,
            language: self.language.clone(),
            generated_at: self.generated_at,
            speakers: self.speakers.clone(),
            segments: self
                .segments
                .iter()
                .map(|segment| TranscriptArtifactSegmentInput {
                    text: segment.text.clone(),
                    start_milliseconds: segment.start_milliseconds,
                    end_milliseconds: segment.end_milliseconds,
                    speaker_id: segment.speaker_id,
                    words: segment.words.clone(),
                })
                .collect(),
        }
    }
}

fn first_16(bytes: [u8; 32]) -> [u8; 16] {
    let mut result = [0_u8; 16];
    result.copy_from_slice(&bytes[..16]);
    result
}
