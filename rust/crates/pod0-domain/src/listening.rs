use crate::{
    AgentCommitId, AgentProposalId, AgentTurnId, ContentDigest, ConversationId, EpisodeId,
    GeneratedArtifactId, PodcastId, QueueEntryId, StateRevision, UnixTimestampMilliseconds,
};

/// Versioned comparison identity matching the current Swift store exactly:
/// lowercase the complete absolute URL without trimming a trailing slash.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct FeedIdentityV1 {
    pub source_url: String,
    pub comparison_key: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum PodcastKind {
    Rss,
    Synthetic,
    Unsupported { wire_code: u32 },
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct PodcastRecord {
    pub podcast_id: PodcastId,
    pub kind: PodcastKind,
    pub feed_identity: Option<FeedIdentityV1>,
    pub title: String,
    pub author: String,
    pub image_url: Option<String>,
    pub description: String,
    pub language: Option<String>,
    pub categories: Vec<String>,
    pub discovered_at: UnixTimestampMilliseconds,
    pub title_is_placeholder: bool,
    pub last_refreshed_at: Option<UnixTimestampMilliseconds>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum AutoDownloadMode {
    Off,
    Latest { count: u16 },
    AllNew,
    Unsupported { wire_code: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Record)]
pub struct AutoDownloadPolicy {
    pub mode: AutoDownloadMode,
    pub wifi_only: bool,
}

/// Controls when transcript work becomes eligible for a followed podcast.
/// Manual user and agent requests remain independent of this background policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum TranscriptStartPolicy {
    Automatic,
    WhenPlayed,
    Unsupported { wire_code: u32 },
}

/// Integer thousandths avoid platform floating-point drift at the boundary.
/// 1.7x is represented as 1700.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Record)]
pub struct PlaybackRatePermille {
    pub value: u16,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct PodcastSubscriptionRecord {
    pub podcast_id: PodcastId,
    pub subscribed_at: UnixTimestampMilliseconds,
    pub auto_download: AutoDownloadPolicy,
    pub notifications_enabled: bool,
    pub default_playback_rate: Option<PlaybackRatePermille>,
    pub transcript_start_policy: TranscriptStartPolicy,
}

/// Opaque durable artifact identity. Payloads and host file URLs do not cross
/// this domain boundary; their owning workflow resolves this versioned key.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ArtifactReference {
    pub schema_version: u32,
    pub opaque_key: String,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum DownloadArtifactStatus {
    Unavailable,
    Available {
        reference: ArtifactReference,
        byte_count: u64,
    },
    Unsupported {
        wire_code: u32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, uniffi::Enum)]
pub enum TranscriptSource {
    Publisher,
    Scribe,
    Whisper,
    OnDevice,
    AssemblyAi,
    Other,
    Unsupported { wire_code: u32 },
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum TranscriptArtifactStatus {
    Unavailable,
    Available {
        reference: ArtifactReference,
        source: TranscriptSource,
    },
    Unsupported {
        wire_code: u32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum CompletionCause {
    NaturalEnd,
    ExplicitUserAction,
    LegacyPlayedFlag,
    Unsupported { wire_code: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum CompletionStatus {
    InProgress,
    Completed { cause: CompletionCause },
    Unsupported { wire_code: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Record)]
pub struct EpisodeListeningState {
    pub resume_position_milliseconds: u64,
    pub completion: CompletionStatus,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct GeneratedAudioArtifactProvenance {
    pub artifact_id: GeneratedArtifactId,
    pub conversation_id: ConversationId,
    pub turn_id: AgentTurnId,
    pub proposal_id: AgentProposalId,
    pub commit_id: AgentCommitId,
    pub media_content_digest: ContentDigest,
    pub script_content_digest: ContentDigest,
    pub media_byte_count: u64,
    pub voice_id: Option<String>,
    pub model_reference: String,
    pub committed_at: UnixTimestampMilliseconds,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct EpisodeRecord {
    pub episode_id: EpisodeId,
    pub podcast_id: PodcastId,
    /// Publisher GUID or the deterministic Swift `synth::` fallback. Exact,
    /// case-sensitive matching is scoped to the parent podcast.
    pub publisher_guid: String,
    pub title: String,
    pub description: String,
    pub published_at: UnixTimestampMilliseconds,
    pub duration_milliseconds: Option<u64>,
    pub enclosure_url: String,
    pub enclosure_mime_type: Option<String>,
    pub image_url: Option<String>,
    pub feed_metadata: EpisodeFeedMetadata,
    pub listening: EpisodeListeningState,
    pub is_starred: bool,
    pub download: DownloadArtifactStatus,
    pub transcript: TranscriptArtifactStatus,
    pub generated_audio: Option<GeneratedAudioArtifactProvenance>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum PublisherTranscriptFormat {
    Json,
    WebVtt,
    SubRip,
    Html,
    PlainText,
    Unknown,
    Unsupported { wire_code: u32 },
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct PublisherTranscriptReference {
    pub url: String,
    pub media_type: Option<String>,
    pub format: PublisherTranscriptFormat,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct PodcastPersonRecord {
    pub name: String,
    pub role: Option<String>,
    pub group: Option<String>,
    pub image_url: Option<String>,
    pub link_url: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct PodcastSoundBiteRecord {
    pub start_milliseconds: u64,
    pub duration_milliseconds: u64,
    pub title: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, uniffi::Record)]
pub struct EpisodeFeedMetadata {
    pub publisher_transcript: Option<PublisherTranscriptReference>,
    pub chapters_url: Option<String>,
    pub persons: Vec<PodcastPersonRecord>,
    pub sound_bites: Vec<PodcastSoundBiteRecord>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Record)]
pub struct PlaybackSegment {
    pub start_position_milliseconds: Option<u64>,
    pub end_position_milliseconds: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct QueueEntry {
    /// Slot identity is independent of episode identity, allowing the same
    /// episode to appear as multiple non-adjacent bounded segments.
    pub queue_entry_id: QueueEntryId,
    pub episode_id: EpisodeId,
    pub segment: Option<PlaybackSegment>,
    pub label: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum PlaybackSleepMode {
    Off,
    Duration { duration_milliseconds: u64 },
    EndOfEpisode,
    Unsupported { wire_code: u32 },
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ListeningPlaybackPolicy {
    pub active_episode_id: Option<EpisodeId>,
    pub active_segment: Option<PlaybackSegment>,
    pub active_label: Option<String>,
    pub queue: Vec<QueueEntry>,
    pub rate: PlaybackRatePermille,
    pub sleep_mode: PlaybackSleepMode,
    pub auto_mark_played_at_natural_end: bool,
    pub auto_play_next: bool,
    pub revision: StateRevision,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ListeningDomainSnapshot {
    pub podcasts: Vec<PodcastRecord>,
    pub subscriptions: Vec<PodcastSubscriptionRecord>,
    pub episodes: Vec<EpisodeRecord>,
    pub playback: ListeningPlaybackPolicy,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct PodcastIdentityRecord {
    pub podcast_id: PodcastId,
    pub feed_identity: FeedIdentityV1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum PodcastIdentityResolution {
    AcceptIncoming { podcast_id: PodcastId },
    PreserveExisting { podcast_id: PodcastId },
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct EpisodeIdentityRecord {
    pub episode_id: EpisodeId,
    pub podcast_id: PodcastId,
    pub publisher_guid: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum EpisodeIdentityResolution {
    AcceptIncoming { episode_id: EpisodeId },
    PreserveExisting { episode_id: EpisodeId },
}
