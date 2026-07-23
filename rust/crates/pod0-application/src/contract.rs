use pod0_domain::{
    AutoDownloadPolicy, CancellationId, ChapterArtifactInput, ClipId, ClipRevision, ClipSource,
    CommandId, ContentDigest, EpisodeId, NoteAuthor, NoteId, NoteKind, NoteRevision, NoteTarget,
    PodcastId, RecallConfigurationInput, SpeakerId, StateRevision, TranscriptArtifactInput,
    UnixTimestampMilliseconds,
};

use crate::{
    EvidenceChunkPolicy, PlaybackCommand, RecallQuery, TranscriptEvidenceInput,
    TranscriptWorkflowConfiguration, TranscriptWorkflowOrigin,
};

pub const FACADE_CONTRACT_VERSION: u32 = 45;
pub const MAX_PROJECTION_ITEMS: u16 = 200;
pub const MAX_OPERATION_ITEMS: usize = 32;
pub const MAX_HOST_REQUEST_BATCH: u16 = 64;

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct CommandEnvelope {
    pub command_id: CommandId,
    pub cancellation_id: CancellationId,
    pub expected_revision: Option<StateRevision>,
    pub command: ApplicationCommand,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct SyntheticPodcastInput {
    /// `None` creates a new stable ID derived from the command identity.
    /// Updates and named built-ins provide their existing stable ID.
    pub podcast_id: Option<PodcastId>,
    pub title: String,
    pub author: String,
    pub image_url: Option<String>,
    pub description: String,
    pub language: Option<String>,
    pub categories: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ExternalEpisodeInput {
    pub podcast_id: PodcastId,
    pub feed_url: Option<String>,
    pub podcast_title: String,
    pub audio_url: String,
    pub title: String,
    pub description: String,
    pub published_at: UnixTimestampMilliseconds,
    pub enclosure_mime_type: Option<String>,
    pub image_url: Option<String>,
    pub duration_milliseconds: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum ApplicationCommand {
    SubscribeToFeed {
        feed_url: String,
    },
    EnsurePodcast {
        feed_url: String,
    },
    RefreshPodcast {
        podcast_id: PodcastId,
    },
    HydratePodcastMetadata {
        podcast_id: PodcastId,
    },
    UpsertSyntheticPodcast {
        podcast: SyntheticPodcastInput,
    },
    UpsertExternalEpisode {
        episode: ExternalEpisodeInput,
    },
    Unsubscribe {
        podcast_id: PodcastId,
    },
    SetSubscriptionNotifications {
        podcast_id: PodcastId,
        enabled: bool,
    },
    SetSubscriptionAutoDownload {
        podcast_id: PodcastId,
        policy: AutoDownloadPolicy,
    },
    SetEpisodeStarred {
        episode_id: EpisodeId,
        starred: bool,
    },
    RequestEpisodeDownload {
        episode_id: EpisodeId,
        origin: crate::DownloadIntentOrigin,
    },
    ReportAutomaticDownloadCandidates {
        podcast_id: PodcastId,
        episode_ids: Vec<EpisodeId>,
    },
    CancelEpisodeDownload {
        episode_id: EpisodeId,
        expected_workflow_revision: StateRevision,
    },
    RemoveEpisodeDownload {
        episode_id: EpisodeId,
        expected_workflow_revision: StateRevision,
    },
    ObserveDownloadEnvironment {
        observation: crate::DownloadEnvironmentObservation,
    },
    ResetListeningData,
    RequestPlayback {
        episode_id: EpisodeId,
    },
    Playback {
        command: PlaybackCommand,
    },
    RecallQuery {
        query: RecallQuery,
    },
    ImportLegacyRecallConfiguration {
        configuration: RecallConfigurationInput,
        source_generation: ContentDigest,
    },
    SetRecallConfiguration {
        expected_configuration_revision: StateRevision,
        configuration: RecallConfigurationInput,
    },
    RebuildTranscriptEvidence {
        input: TranscriptEvidenceInput,
        policy: EvidenceChunkPolicy,
    },
    CommitRecallIndexCutover,
    CommitTranscript {
        expected_selection_revision: StateRevision,
        artifact: TranscriptArtifactInput,
    },
    EnsureTranscriptWorkflow {
        episode_id: EpisodeId,
        origin: TranscriptWorkflowOrigin,
        configuration: TranscriptWorkflowConfiguration,
    },
    RetryTranscriptWorkflow {
        episode_id: EpisodeId,
        expected_workflow_revision: StateRevision,
        configuration: TranscriptWorkflowConfiguration,
    },
    CancelTranscriptWorkflow {
        episode_id: EpisodeId,
        expected_workflow_revision: StateRevision,
    },
    EnsureScheduledTask {
        task: crate::ScheduledTaskInput,
    },
    UpdateScheduledTask {
        task_id: pod0_domain::ScheduledTaskId,
        expected_task_revision: StateRevision,
        task: crate::ScheduledTaskInput,
    },
    RemoveScheduledTask {
        task_id: pod0_domain::ScheduledTaskId,
        expected_task_revision: StateRevision,
    },
    ReconcileScheduledRuns,
    RetryScheduledRun {
        occurrence_id: pod0_domain::ScheduledOccurrenceId,
        expected_workflow_revision: StateRevision,
    },
    CancelScheduledRun {
        occurrence_id: pod0_domain::ScheduledOccurrenceId,
        expected_workflow_revision: StateRevision,
    },
    StartAgentTurn {
        conversation_id: Option<pod0_domain::ConversationId>,
        user_input: String,
        model_reference: String,
    },
    PublishGeneratedEpisode {
        intent: pod0_domain::PublicationIntent,
    },
    EnsureNostrSigner,
    SignOutNostrSigner {
        expected_account_id: pod0_domain::SignerAccountId,
    },
    CancelAgentTurn {
        turn_id: pod0_domain::AgentTurnId,
        expected_turn_revision: StateRevision,
    },
    CommitChapter {
        expected_selection_revision: StateRevision,
        artifact: ChapterArtifactInput,
    },
    EnsurePublisherChapters {
        episode_id: EpisodeId,
    },
    RetryPublisherChapters {
        episode_id: EpisodeId,
        expected_workflow_revision: StateRevision,
    },
    CancelPublisherChapters {
        episode_id: EpisodeId,
        expected_workflow_revision: StateRevision,
    },
    EnsureModelChapters {
        episode_id: EpisodeId,
        configured_model: String,
    },
    RetryModelChapters {
        episode_id: EpisodeId,
        configured_model: String,
        expected_workflow_revision: StateRevision,
    },
    CancelModelChapters {
        episode_id: EpisodeId,
        expected_workflow_revision: StateRevision,
    },
    CreateNote {
        text: String,
        kind: NoteKind,
        author: NoteAuthor,
        target: Option<NoteTarget>,
    },
    UpdateNote {
        note_id: NoteId,
        expected_note_revision: NoteRevision,
        text: String,
        kind: NoteKind,
        target: Option<NoteTarget>,
    },
    SetNoteDeleted {
        note_id: NoteId,
        expected_note_revision: NoteRevision,
        deleted: bool,
    },
    ClearNotes {
        expected_collection_revision: StateRevision,
    },
    CreateMemory {
        content: String,
    },
    UpdateMemory {
        memory_id: pod0_domain::MemoryId,
        expected_memory_revision: pod0_domain::MemoryRevision,
        content: String,
    },
    SetMemoryDeleted {
        memory_id: pod0_domain::MemoryId,
        expected_memory_revision: pod0_domain::MemoryRevision,
        deleted: bool,
    },
    ClearMemories {
        expected_collection_revision: StateRevision,
    },
    CreateClip {
        clip_id: ClipId,
        episode_id: EpisodeId,
        podcast_id: PodcastId,
        start_milliseconds: u64,
        end_milliseconds: u64,
        caption: Option<String>,
        speaker_id: Option<SpeakerId>,
        frozen_transcript_text: String,
        source: ClipSource,
    },
    UpdateClip {
        clip_id: ClipId,
        expected_clip_revision: ClipRevision,
        start_milliseconds: u64,
        end_milliseconds: u64,
        caption: Option<String>,
        speaker_id: Option<SpeakerId>,
        frozen_transcript_text: String,
    },
    SetClipDeleted {
        clip_id: ClipId,
        expected_clip_revision: ClipRevision,
        deleted: bool,
    },
    ClearClips {
        expected_collection_revision: StateRevision,
    },
    CancelOperation {
        cancellation_id: CancellationId,
    },
    Unsupported {
        wire_code: u32,
    },
}
