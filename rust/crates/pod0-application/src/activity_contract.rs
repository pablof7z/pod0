use pod0_domain::{
    ActivityCorrelationId, ActivityId, ActivityTransactionId, AgentTurnId, ClipId, CommandId,
    ConversationId, EffectAttemptId, EffectIntentId, EpisodeId, HostRequestId, InternalCommandId,
    MemoryId, NoteId, PodcastId, PublicationId, ScheduledOccurrenceId, SpeakerEntityId,
    StateRevision, TranscriptArtifactId, TranscriptWorkflowId, UnixTimestampMilliseconds,
};

use crate::DomainTransitionKind;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ActivityActor {
    User,
    System,
    Agent,
    Recovery,
    Migration,
    Unsupported { wire_code: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ActivityOrigin {
    UserInterface,
    AutomaticPolicy,
    Playback,
    AgentTool,
    ScheduledWork,
    HostObservation,
    Recovery,
    Migration,
    InternalCommand,
    Unsupported { wire_code: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ActivityDomain {
    LibraryFeed,
    Playback,
    Download,
    Transcript,
    Chapter,
    RecallKnowledge,
    ScheduledAgent,
    AgentPublication,
    UserArtifact,
    Lifecycle,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ActivitySubject {
    Global,
    Podcast {
        podcast_id: PodcastId,
    },
    Episode {
        episode_id: EpisodeId,
    },
    Conversation {
        conversation_id: ConversationId,
    },
    AgentTurn {
        turn_id: AgentTurnId,
    },
    ScheduledOccurrence {
        occurrence_id: ScheduledOccurrenceId,
    },
    TranscriptWorkflow {
        workflow_id: TranscriptWorkflowId,
    },
    Publication {
        publication_id: PublicationId,
    },
    Note {
        note_id: NoteId,
    },
    Memory {
        memory_id: MemoryId,
    },
    Clip {
        clip_id: ClipId,
    },
    SpeakerEntity {
        speaker_entity_id: SpeakerEntityId,
    },
    TranscriptArtifact {
        artifact_id: TranscriptArtifactId,
    },
    Operation {
        command_id: CommandId,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum RequestRejectionReason {
    Invalid,
    NotAllowed,
    RevisionConflict,
    MissingSubject,
    Unsupported,
    StorageUnavailable,
    PrivacyBoundary,
    MissingPrerequisite,
    UnsupportedCode { wire_code: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum RequestDisposition {
    Accepted,
    Rejected { reason: RequestRejectionReason },
    Stale,
    Duplicate,
    AlreadyComplete,
    NoSemanticChange,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ExternalEffectKind {
    FeedNetwork,
    Playback,
    RecallProvider,
    /// Legacy read-only wire value. New chapter work must use one of the typed kinds below.
    ChapterProvider,
    Download,
    Notification,
    TranscriptProvider,
    AgentProvider,
    AgentApproval,
    AgentCapability,
    ScheduledAgentProvider,
    CoreWake,
    Filesystem,
    Publication,
    PublisherChapterProvider,
    ModelChapterProvider,
    Cancellation,
    LibraryNetwork,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ActivityFailureCode {
    Offline,
    TimedOut,
    PermissionDenied,
    InvalidResponse,
    ResponseTooLarge,
    MediaUnavailable,
    ProviderUnavailable,
    Unauthorized,
    StorageUnavailable,
    PlatformFailure,
    Unsupported { wire_code: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum EffectOutcome {
    Progressed,
    Succeeded,
    Failed { code: ActivityFailureCode },
    Cancelled,
    Superseded,
    OutcomeUnknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ActivityFact {
    RequestDisposition {
        disposition: RequestDisposition,
    },
    DomainTransition {
        kind: DomainTransitionKind,
        previous_revision: StateRevision,
        committed_revision: StateRevision,
    },
    PlaybackCheckpoint {
        position_milliseconds: u64,
    },
    EffectAuthorized {
        intent_id: EffectIntentId,
        kind: ExternalEffectKind,
    },
    EffectObserved {
        intent_id: EffectIntentId,
        attempt_id: EffectAttemptId,
        outcome: EffectOutcome,
    },
    InternalCommandAuthorized {
        internal_command_id: InternalCommandId,
        target: ActivityDomain,
    },
    RecoveryTransition {
        outcome: EffectOutcome,
    },
    AuthorityCutover {
        domain: ActivityDomain,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ActivityFactDraft {
    pub activity_id: ActivityId,
    pub transaction_id: ActivityTransactionId,
    pub correlation_id: ActivityCorrelationId,
    pub caused_by_activity_id: Option<ActivityId>,
    pub command_id: Option<CommandId>,
    pub host_request_id: Option<HostRequestId>,
    pub actor: ActivityActor,
    pub origin: ActivityOrigin,
    pub subject: ActivitySubject,
    pub episode_id: Option<EpisodeId>,
    pub fact: ActivityFact,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CommittedActivityFact {
    pub sequence: u64,
    pub committed_at: UnixTimestampMilliseconds,
    pub draft: ActivityFactDraft,
}
