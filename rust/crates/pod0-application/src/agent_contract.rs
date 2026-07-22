use pod0_domain::{
    AgentCommitId, AgentExecutionFenceId, AgentProposalId, AgentTurnId, ContentDigest,
    ConversationId, EpisodeId, GeneratedArtifactId, PodcastId, ScheduledTaskId, StateRevision,
    UnixTimestampMilliseconds,
};

use crate::{AgentToolName, QueuePlacement, ScheduledTaskInput};

pub const AGENT_CONTRACT_VERSION: u32 = 1;
pub const MAX_AGENT_INPUT_BYTES: usize = 32 * 1_024;
pub const MAX_AGENT_MESSAGE_BYTES: usize = 64 * 1_024;
pub const MAX_AGENT_MODEL_REFERENCE_BYTES: usize = 256;
pub const MAX_AGENT_ACTION_TEXT_BYTES: usize = 64 * 1_024;
pub const MAX_AGENT_PROJECTION_MESSAGES: usize = 64;
pub const MAX_AGENT_SAFE_DETAIL_BYTES: usize = 1_024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum AgentAuthority {
    None,
    DurableTurnGrant,
    DurableScopedGrant,
    OneShotApproval,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum AgentToolClass {
    ReadOnly,
    ReversibleWrite,
    ExternalSideEffect,
    DestructiveWrite,
    SecretBearing,
    Publication,
    SessionLocal,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct AgentToolPolicy {
    pub tool: AgentToolName,
    pub classes: Vec<AgentToolClass>,
    pub authority: AgentAuthority,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum AgentToolAction {
    NoArguments {
        tool: AgentToolName,
    },
    TextInput {
        tool: AgentToolName,
        text: String,
    },
    Search {
        tool: AgentToolName,
        query: String,
        scope: Option<String>,
        limit: u16,
    },
    Episode {
        tool: AgentToolName,
        episode_id: EpisodeId,
    },
    Podcast {
        tool: AgentToolName,
        podcast_id: PodcastId,
    },
    PlayEpisode {
        episode_id: EpisodeId,
        start_milliseconds: Option<u64>,
        end_milliseconds: Option<u64>,
        placement: QueuePlacement,
    },
    SetPlaybackRate {
        permille: u16,
    },
    SetSleepTimer {
        duration_milliseconds: Option<u64>,
    },
    CreateNote {
        text: String,
    },
    RecordMemory {
        text: String,
    },
    Ask {
        question: String,
        context: Option<String>,
    },
    ScheduleTask {
        task: ScheduledTaskInput,
    },
    CancelScheduledTask {
        task_id: ScheduledTaskId,
        expected_revision: StateRevision,
    },
    ChangePodcastCategory {
        podcast_id: PodcastId,
        category: String,
    },
    CreateClip {
        episode_id: EpisodeId,
        podcast_id: PodcastId,
        start_milliseconds: u64,
        end_milliseconds: u64,
        caption: Option<String>,
        frozen_transcript_text: String,
    },
    SubscribePodcast {
        feed_url: String,
    },
    IngestYoutubeVideo {
        url: String,
    },
    ConfigureAgentVoice {
        voice_id: String,
    },
    CreatePodcast {
        title: String,
        description: String,
    },
    UpdatePodcast {
        podcast_id: PodcastId,
        title: String,
        description: String,
    },
    GenerateTtsEpisode {
        podcast_id: Option<PodcastId>,
        title: String,
        script: String,
        voice_id: Option<String>,
    },
    GeneratePodcastArtwork {
        podcast_id: PodcastId,
        prompt: String,
    },
}

impl AgentToolAction {
    #[must_use]
    pub const fn tool(&self) -> AgentToolName {
        match self {
            Self::NoArguments { tool }
            | Self::TextInput { tool, .. }
            | Self::Search { tool, .. }
            | Self::Episode { tool, .. }
            | Self::Podcast { tool, .. } => *tool,
            Self::PlayEpisode { .. } => AgentToolName::PlayEpisode,
            Self::SetPlaybackRate { .. } => AgentToolName::SetPlaybackRate,
            Self::SetSleepTimer { .. } => AgentToolName::SetSleepTimer,
            Self::CreateNote { .. } => AgentToolName::CreateNote,
            Self::RecordMemory { .. } => AgentToolName::RecordMemory,
            Self::Ask { .. } => AgentToolName::Ask,
            Self::ScheduleTask { .. } => AgentToolName::ScheduleTask,
            Self::CancelScheduledTask { .. } => AgentToolName::CancelScheduledTask,
            Self::ChangePodcastCategory { .. } => AgentToolName::ChangePodcastCategory,
            Self::CreateClip { .. } => AgentToolName::CreateClip,
            Self::SubscribePodcast { .. } => AgentToolName::SubscribePodcast,
            Self::IngestYoutubeVideo { .. } => AgentToolName::IngestYoutubeVideo,
            Self::ConfigureAgentVoice { .. } => AgentToolName::ConfigureAgentVoice,
            Self::CreatePodcast { .. } => AgentToolName::CreatePodcast,
            Self::UpdatePodcast { .. } => AgentToolName::UpdatePodcast,
            Self::GenerateTtsEpisode { .. } => AgentToolName::GenerateTtsEpisode,
            Self::GeneratePodcastArtwork { .. } => AgentToolName::GeneratePodcastArtwork,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum AgentTurnStage {
    AwaitingModel,
    ApprovalRequired,
    Authorized,
    Executing,
    CommitPending,
    Committed,
    Completed,
    Denied,
    Cancelled,
    Blocked,
    OutcomeAmbiguous,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum AgentMessageRole {
    User,
    Assistant,
    Tool,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct AgentMessageProjection {
    pub role: AgentMessageRole,
    pub content: String,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct AgentProposalProjection {
    pub proposal_id: AgentProposalId,
    pub proposal_digest: ContentDigest,
    pub revision: StateRevision,
    pub action: AgentToolAction,
    pub required_authority: AgentAuthority,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct AgentCommitReceipt {
    pub commit_id: AgentCommitId,
    pub proposal_id: AgentProposalId,
    pub artifact_id: Option<GeneratedArtifactId>,
    pub committed_at: UnixTimestampMilliseconds,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct AgentTurnProjection {
    pub conversation_id: ConversationId,
    pub turn_id: AgentTurnId,
    pub revision: StateRevision,
    pub stage: AgentTurnStage,
    pub messages: Vec<AgentMessageProjection>,
    pub proposal: Option<AgentProposalProjection>,
    pub execution_fence_id: Option<AgentExecutionFenceId>,
    pub commit: Option<AgentCommitReceipt>,
    pub safe_failure: Option<String>,
    pub updated_at: UnixTimestampMilliseconds,
}

impl AgentTurnProjection {
    pub fn enforce_bounds(&mut self, requested_items: usize) {
        let limit = requested_items.clamp(1, MAX_AGENT_PROJECTION_MESSAGES);
        if self.messages.len() > limit {
            self.messages.drain(..self.messages.len() - limit);
        }
    }
}
