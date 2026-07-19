use pod0_domain::{
    CommandId, ContentDigest, EpisodeId, PodcastId, SpeakerId, StateRevision, TranscriptArtifactId,
    TranscriptSegmentId, TranscriptSource, TranscriptVersionId, UnixTimestampMilliseconds,
};

pub const MAX_TRANSCRIPT_PROJECTION_ITEMS: u16 = 200;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TranscriptCommitStorageReceipt {
    pub command_id: CommandId,
    pub artifact_id: TranscriptArtifactId,
    pub transcript_version_id: TranscriptVersionId,
    pub transcript_content_digest: ContentDigest,
    pub artifact_integrity_digest: ContentDigest,
    pub command_fingerprint: ContentDigest,
    pub previous_artifact_id: Option<TranscriptArtifactId>,
    pub selection_revision: StateRevision,
    pub speaker_count: u32,
    pub segment_count: u32,
    pub word_count: u64,
    pub already_selected: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TranscriptSelectionSummary {
    pub artifact_id: TranscriptArtifactId,
    pub transcript_version_id: TranscriptVersionId,
    pub episode_id: EpisodeId,
    pub podcast_id: PodcastId,
    pub source_revision: String,
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredTranscriptSpeaker {
    pub speaker_id: SpeakerId,
    pub ordinal: u32,
    pub label: String,
    pub display_name: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredTranscriptSegment {
    pub segment_id: TranscriptSegmentId,
    pub ordinal: u32,
    pub text: String,
    pub start_milliseconds: u64,
    pub end_milliseconds: u64,
    pub speaker_id: Option<SpeakerId>,
    pub word_count: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredTranscriptWord {
    pub segment_id: TranscriptSegmentId,
    pub ordinal: u32,
    pub text: String,
    pub start_milliseconds: u64,
    pub end_milliseconds: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TranscriptPage<T> {
    pub items: Vec<T>,
    pub has_more: bool,
}
