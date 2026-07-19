use pod0_domain::{
    ContentDigest, EpisodeId, PodcastId, SpeakerId, StateRevision, TranscriptArtifact,
    TranscriptArtifactError, TranscriptArtifactId, TranscriptArtifactInput, TranscriptSegmentId,
    TranscriptSource, TranscriptVersionId, UnixTimestampMilliseconds,
};
use sha2::{Digest as _, Sha256};

use crate::OperationProjection;

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct TranscriptCommitRequest {
    pub command_id: pod0_domain::CommandId,
    pub expected_selection_revision: StateRevision,
    pub artifact: TranscriptArtifactInput,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Record)]
pub struct TranscriptCommitReceipt {
    pub command_id: pod0_domain::CommandId,
    pub artifact_id: TranscriptArtifactId,
    pub transcript_version_id: TranscriptVersionId,
    pub transcript_content_digest: ContentDigest,
    pub artifact_integrity_digest: ContentDigest,
    pub command_fingerprint: ContentDigest,
    pub selection_revision: StateRevision,
    pub speaker_count: u32,
    pub segment_count: u32,
    pub word_count: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum TranscriptProjectionScope {
    Summary,
    Speakers,
    Segments,
    Segment { segment_id: TranscriptSegmentId },
    Words { segment_id: TranscriptSegmentId },
    Unsupported { wire_code: u32 },
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct TranscriptSummaryProjection {
    pub artifact_id: TranscriptArtifactId,
    pub transcript_version_id: TranscriptVersionId,
    pub episode_id: EpisodeId,
    pub podcast_id: PodcastId,
    pub source: TranscriptSource,
    pub provider: Option<String>,
    pub source_payload_digest: ContentDigest,
    pub language: String,
    pub generated_at: UnixTimestampMilliseconds,
    pub transcript_content_digest: ContentDigest,
    pub artifact_integrity_digest: ContentDigest,
    pub selection_revision: StateRevision,
    pub speaker_count: u32,
    pub segment_count: u32,
    pub word_count: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct TranscriptSpeakerProjection {
    pub speaker_id: SpeakerId,
    pub label: String,
    pub display_name: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct TranscriptSegmentProjection {
    pub segment_id: TranscriptSegmentId,
    pub ordinal: u32,
    pub text: String,
    pub start_milliseconds: u64,
    pub end_milliseconds: u64,
    pub speaker_id: Option<SpeakerId>,
    pub word_count: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct TranscriptWordProjection {
    pub segment_id: TranscriptSegmentId,
    pub ordinal: u32,
    pub text: String,
    pub start_milliseconds: u64,
    pub end_milliseconds: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct TranscriptProjection {
    pub scope: TranscriptProjectionScope,
    pub summary: Option<TranscriptSummaryProjection>,
    pub speakers: Vec<TranscriptSpeakerProjection>,
    pub segments: Vec<TranscriptSegmentProjection>,
    pub words: Vec<TranscriptWordProjection>,
    pub operations: Vec<OperationProjection>,
    pub has_more: bool,
}

/// Bounded state returned by the pure pre-cutover contract projection.
/// Rejections are data, never an exception crossing UniFFI.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
// UniFFI enum payloads are value records. The qualified transcript is already
// page-bounded; boxing only this Rust variant would not reduce FFI payload work.
#[allow(clippy::large_enum_variant)]
pub enum TranscriptContractProjection {
    Qualified {
        receipt: TranscriptCommitReceipt,
        transcript: TranscriptProjection,
    },
    Rejected {
        reason: TranscriptContractRejection,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum TranscriptContractRejection {
    InvalidMetadata,
    InvalidSpeaker,
    InvalidSegment,
    InvalidWord,
    CollectionLimit,
    TextLimit,
    IdentityMismatch,
    RevisionExhausted,
    Unsupported { wire_code: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TranscriptContractError {
    InvalidMetadata,
    InvalidSpeaker,
    InvalidSegment,
    InvalidWord,
    CollectionLimit,
    TextLimit,
    IdentityMismatch,
    RevisionExhausted,
}

impl std::fmt::Display for TranscriptContractError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "invalid transcript contract: {self:?}")
    }
}

impl std::error::Error for TranscriptContractError {}

/// Produces state-shaped, bounded contract evidence for native binding
/// compatibility checks. Invalid input is represented by `Rejected`; this
/// function cannot throw across UniFFI and does not mutate durable state.
#[must_use]
pub fn project_transcript_contract(
    request: TranscriptCommitRequest,
    scope: TranscriptProjectionScope,
    offset: u32,
    max_items: u16,
) -> TranscriptContractProjection {
    match qualify_transcript(request) {
        Ok((receipt, artifact)) => {
            let mut transcript =
                crate::project_transcript_artifact(&artifact, receipt.selection_revision, scope);
            transcript.enforce_bounds(
                usize::try_from(offset).unwrap_or(usize::MAX),
                usize::from(max_items),
            );
            TranscriptContractProjection::Qualified {
                receipt,
                transcript,
            }
        }
        Err(error) => TranscriptContractProjection::Rejected {
            reason: error.into(),
        },
    }
}

pub fn qualify_transcript_commit(
    request: TranscriptCommitRequest,
) -> Result<TranscriptCommitReceipt, TranscriptContractError> {
    qualify_transcript(request).map(|(receipt, _)| receipt)
}

fn qualify_transcript(
    request: TranscriptCommitRequest,
) -> Result<(TranscriptCommitReceipt, TranscriptArtifact), TranscriptContractError> {
    let selection_revision = StateRevision::new(
        request
            .expected_selection_revision
            .value
            .checked_add(1)
            .ok_or(TranscriptContractError::RevisionExhausted)?,
    );
    let command_id = request.command_id;
    let artifact =
        TranscriptArtifact::seal(request.artifact).map_err(TranscriptContractError::from)?;
    let speaker_count = u32::try_from(artifact.speakers.len())
        .map_err(|_| TranscriptContractError::CollectionLimit)?;
    let segment_count = u32::try_from(artifact.segments.len())
        .map_err(|_| TranscriptContractError::CollectionLimit)?;
    let word_count = artifact
        .segments
        .iter()
        .map(|segment| segment.words.len() as u64)
        .sum();
    let receipt = TranscriptCommitReceipt {
        command_id,
        artifact_id: artifact.artifact_id,
        transcript_version_id: artifact.transcript_version_id,
        transcript_content_digest: artifact.content_digest,
        artifact_integrity_digest: artifact.integrity_digest,
        command_fingerprint: transcript_command_fingerprint(
            request.expected_selection_revision,
            &artifact,
        ),
        selection_revision,
        speaker_count,
        segment_count,
        word_count,
    };
    Ok((receipt, artifact))
}

pub fn qualify_transcript_projection(
    request: TranscriptCommitRequest,
    scope: TranscriptProjectionScope,
    offset: u32,
    max_items: u16,
) -> Result<TranscriptProjection, TranscriptContractError> {
    let selection_revision = StateRevision::new(
        request
            .expected_selection_revision
            .value
            .checked_add(1)
            .ok_or(TranscriptContractError::RevisionExhausted)?,
    );
    let artifact =
        TranscriptArtifact::seal(request.artifact).map_err(TranscriptContractError::from)?;
    let mut projection = crate::project_transcript_artifact(&artifact, selection_revision, scope);
    projection.enforce_bounds(
        usize::try_from(offset).unwrap_or(usize::MAX),
        usize::from(max_items),
    );
    Ok(projection)
}

fn transcript_command_fingerprint(
    expected_revision: StateRevision,
    artifact: &TranscriptArtifact,
) -> ContentDigest {
    let mut hash = Sha256::new();
    hash.update(b"pod0.commit-transcript.v1\0");
    hash.update(expected_revision.value.to_be_bytes());
    hash.update(artifact.artifact_id.into_bytes());
    hash.update(artifact.integrity_digest.into_bytes());
    ContentDigest::from_bytes(hash.finalize().into())
}

impl From<TranscriptArtifactError> for TranscriptContractError {
    fn from(value: TranscriptArtifactError) -> Self {
        use TranscriptArtifactError as Domain;
        match value {
            Domain::InvalidSourceRevision | Domain::InvalidProvider | Domain::InvalidLanguage => {
                Self::InvalidMetadata
            }
            Domain::DuplicateSpeaker | Domain::InvalidSpeakerLabel => Self::InvalidSpeaker,
            Domain::InvalidSegmentText
            | Domain::InvalidSegmentTime
            | Domain::SegmentsOutOfOrder => Self::InvalidSegment,
            Domain::InvalidWordText | Domain::InvalidWordTime | Domain::WordsOutOfOrder => {
                Self::InvalidWord
            }
            Domain::TooManySpeakers | Domain::TooManySegments | Domain::TooManyWords => {
                Self::CollectionLimit
            }
            Domain::SegmentTextTooLong | Domain::TranscriptTooLarge => Self::TextLimit,
            Domain::IdentityMismatch => Self::IdentityMismatch,
        }
    }
}

impl From<TranscriptContractError> for TranscriptContractRejection {
    fn from(value: TranscriptContractError) -> Self {
        match value {
            TranscriptContractError::InvalidMetadata => Self::InvalidMetadata,
            TranscriptContractError::InvalidSpeaker => Self::InvalidSpeaker,
            TranscriptContractError::InvalidSegment => Self::InvalidSegment,
            TranscriptContractError::InvalidWord => Self::InvalidWord,
            TranscriptContractError::CollectionLimit => Self::CollectionLimit,
            TranscriptContractError::TextLimit => Self::TextLimit,
            TranscriptContractError::IdentityMismatch => Self::IdentityMismatch,
            TranscriptContractError::RevisionExhausted => Self::RevisionExhausted,
        }
    }
}
