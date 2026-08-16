use pod0_domain::{AgentTurnId, ClipId, EpisodeId, HostRequestId, MemoryId, NoteId};

use crate::{ActivityDomain, ActivitySubject, ExternalEffectKind};

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DurableExternalEffectRequest {
    pub kind: ExternalEffectKind,
    pub subject: ActivitySubject,
    pub episode_id: Option<EpisodeId>,
    pub not_before: Option<pod0_domain::UnixTimestampMilliseconds>,
    pub deadline_at: Option<pod0_domain::UnixTimestampMilliseconds>,
    pub execution: DurableEffectExecution,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum DurableEffectExecution {
    #[serde(rename = "DomainDerived")]
    LegacyDomainDerived,
    Playback {
        request: crate::DurablePlaybackEffectRequest,
    },
    Download {
        request: crate::DurableDownloadEffectRequest,
    },
    Feed {
        request: crate::DurableFeedEffectRequest,
    },
    Publication {
        draft: crate::Pod0PublicationDraft,
    },
    AgentModel {
        request: DurableAgentModelEffectRequest,
    },
    AgentApproval {
        request: DurableAgentApprovalEffectRequest,
    },
    AgentCapability {
        request: DurableAgentCapabilityEffectRequest,
    },
    ScheduledAgent {
        request: DurableScheduledAgentEffectRequest,
    },
    AgentRecall {
        request: crate::DurableAgentRecallEffectRequest,
    },
    RecallQuery {
        request: crate::DurableRecallQueryEffectRequest,
    },
    RecallIndexCutover {
        request: crate::DurableRecallIndexCutoverEffectRequest,
    },
    EvidenceEmbedding {
        request: DurableEvidenceEmbeddingEffectRequest,
    },
    Transcript {
        request: DurableTranscriptEffectRequest,
    },
    PublisherChapter {
        request: DurablePublisherChapterEffectRequest,
    },
    ModelChapter {
        request: DurableModelChapterEffectRequest,
    },
    Lifecycle {
        request: crate::DurableLifecycleEffectRequest,
    },
    Cancellation {
        request: crate::DurableHostCancellationEffectRequest,
    },
    LibraryNetwork {
        request: crate::DurableLibraryNetworkEffectRequest,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DurableTranscriptEffectRequest {
    pub request_id: HostRequestId,
    pub command_id: pod0_domain::CommandId,
    pub cancellation_id: pod0_domain::CancellationId,
    pub issued_revision: pod0_domain::StateRevision,
    pub deadline_at: Option<pod0_domain::UnixTimestampMilliseconds>,
    pub capability: crate::TranscriptCapabilityRequest,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DurablePublisherChapterEffectRequest {
    pub request_id: HostRequestId,
    pub command_id: pod0_domain::CommandId,
    pub cancellation_id: pod0_domain::CancellationId,
    pub issued_revision: pod0_domain::StateRevision,
    pub deadline_at: Option<pod0_domain::UnixTimestampMilliseconds>,
    pub episode_id: EpisodeId,
    pub source_url: String,
    pub not_before: Option<pod0_domain::UnixTimestampMilliseconds>,
    pub maximum_response_bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DurableModelChapterEffectRequest {
    pub request_id: HostRequestId,
    pub command_id: pod0_domain::CommandId,
    pub cancellation_id: pod0_domain::CancellationId,
    pub issued_revision: pod0_domain::StateRevision,
    pub deadline_at: Option<pod0_domain::UnixTimestampMilliseconds>,
    pub episode_id: EpisodeId,
    pub generation: u64,
    pub submission_fence_id: pod0_domain::ChapterModelSubmissionFenceId,
    pub action: DurableModelChapterAction,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum DurableModelChapterAction {
    Execute {
        execution: crate::ChapterModelExecutionRequest,
    },
    Recover {
        provider: String,
        model: String,
        provider_operation_id: String,
        provider_status: Option<String>,
        maximum_completion_bytes: u64,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DurableAgentModelEffectRequest {
    pub request_id: HostRequestId,
    pub command_id: pod0_domain::CommandId,
    pub cancellation_id: pod0_domain::CancellationId,
    pub issued_revision: pod0_domain::StateRevision,
    pub deadline_at: Option<pod0_domain::UnixTimestampMilliseconds>,
    pub execution: crate::AgentModelExecutionRequest,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DurableAgentApprovalEffectRequest {
    pub request_id: HostRequestId,
    pub command_id: pod0_domain::CommandId,
    pub cancellation_id: pod0_domain::CancellationId,
    pub issued_revision: pod0_domain::StateRevision,
    pub deadline_at: Option<pod0_domain::UnixTimestampMilliseconds>,
    pub approval: crate::AgentApprovalRequest,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DurableAgentCapabilityEffectRequest {
    pub request_id: HostRequestId,
    pub command_id: pod0_domain::CommandId,
    pub cancellation_id: pod0_domain::CancellationId,
    pub issued_revision: pod0_domain::StateRevision,
    pub deadline_at: Option<pod0_domain::UnixTimestampMilliseconds>,
    pub capability: crate::AgentCapabilityRequest,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DurableScheduledAgentEffectRequest {
    pub request_id: HostRequestId,
    pub command_id: pod0_domain::CommandId,
    pub cancellation_id: pod0_domain::CancellationId,
    pub issued_revision: pod0_domain::StateRevision,
    pub deadline_at: pod0_domain::UnixTimestampMilliseconds,
    pub execution: crate::ScheduledAgentExecutionRequest,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DurableEvidenceIndexTarget {
    pub episode_id: EpisodeId,
    pub generation_id: pod0_domain::EvidenceGenerationId,
    pub expected_span_count: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum DurableEvidenceIndexCompletion {
    EvidenceRebuild,
    TranscriptWorkflow {
        workflow_id: pod0_domain::TranscriptWorkflowId,
        input_version: String,
    },
    RecallConfiguration {
        imported: Option<bool>,
        revision: pod0_domain::StateRevision,
        completed_episode_count: u32,
        remaining: Vec<DurableEvidenceIndexTarget>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DurableEvidenceEmbeddingEffectRequest {
    pub request_id: HostRequestId,
    pub command_id: pod0_domain::CommandId,
    pub cancellation_id: pod0_domain::CancellationId,
    pub issued_revision: pod0_domain::StateRevision,
    pub deadline_at: pod0_domain::UnixTimestampMilliseconds,
    pub episode_id: EpisodeId,
    pub generation_id: pod0_domain::EvidenceGenerationId,
    pub expected_span_count: u32,
    pub provider: pod0_domain::RecallEmbeddingProvider,
    pub model: String,
    pub spans: Vec<crate::RecallEmbeddingInput>,
    pub completion: DurableEvidenceIndexCompletion,
}

pub trait ExternalEffectRequest {
    fn effect_kind(&self) -> ExternalEffectKind;
    fn subject(&self) -> ActivitySubject;
    fn episode_id(&self) -> Option<EpisodeId>;
}

impl ExternalEffectRequest for DurableExternalEffectRequest {
    fn effect_kind(&self) -> ExternalEffectKind {
        self.kind
    }

    fn subject(&self) -> ActivitySubject {
        self.subject
    }

    fn episode_id(&self) -> Option<EpisodeId> {
        self.episode_id
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum InternalCommandKind {
    BuildTranscriptEvidence,
    EnsureTranscriptWorkflow {
        origin: crate::TranscriptWorkflowOrigin,
        configuration: crate::TranscriptWorkflowConfiguration,
    },
    EnsurePublisherChapters,
    EnsureModelChapters {
        configured_model: String,
    },
    ReconcileScheduledRuns,
    ContinueWorkflowReconciliation {
        opportunity: crate::WorkflowOpportunity,
        episode_offset: u32,
    },
    RequestEpisodeDownload {
        origin: crate::DownloadIntentOrigin,
    },
    FinalizeDownloadArtifact {
        request_id: HostRequestId,
        sequence_number: u64,
        staged_file_path: String,
        claimed_byte_count: u64,
    },
    FinalizeModelChapters {
        request_id: HostRequestId,
    },
    AdvanceAgentTurn {
        turn_id: AgentTurnId,
    },
    ExecuteAgentProjection {
        turn_id: AgentTurnId,
    },
    ExecuteAgentTool {
        turn_id: AgentTurnId,
    },
    CompleteAgentTool {
        turn_id: AgentTurnId,
        completion: AgentToolCompletion,
    },
}

include!("agent_tool_completion.rs");

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DurableInternalCommandRequest {
    pub kind: InternalCommandKind,
    pub target: ActivityDomain,
    pub subject: ActivitySubject,
    pub episode_id: Option<EpisodeId>,
}

pub trait InternalCommandRequest {
    fn target_domain(&self) -> ActivityDomain;
    fn subject(&self) -> ActivitySubject;
    fn episode_id(&self) -> Option<EpisodeId>;
}

impl InternalCommandRequest for DurableInternalCommandRequest {
    fn target_domain(&self) -> ActivityDomain {
        self.target
    }

    fn subject(&self) -> ActivitySubject {
        self.subject
    }

    fn episode_id(&self) -> Option<EpisodeId> {
        self.episode_id
    }
}
