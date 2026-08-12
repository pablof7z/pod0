use crate::{AgentToolName, QueuePlacement, RecallScope, ScheduledTaskInput};
use pod0_domain::{
    CategoryId, EpisodeId, LibraryItemId, PodcastId, ScheduledTaskId, StateRevision,
};
pub const AGENT_CONTRACT_VERSION: u32 = 4;
pub const MAX_AGENT_INPUT_BYTES: usize = 32 * 1_024;
pub const MAX_AGENT_MESSAGE_BYTES: usize = 64 * 1_024;
pub const MAX_AGENT_MODEL_REFERENCE_BYTES: usize = 256;
pub const MAX_AGENT_ACTION_TEXT_BYTES: usize = 64 * 1_024;
pub const MAX_AGENT_PROJECTION_MESSAGES: usize = 64;
pub const MAX_AGENT_SAFE_DETAIL_BYTES: usize = 1_024;
/// A category name is a navigation label, not prose. Bounded tightly so the
/// model cannot write a paragraph into a swipe-page title.
pub const MAX_AGENT_CATEGORY_NAME_BYTES: usize = 128;
/// One or two sentences describing what belongs in a category.
pub const MAX_AGENT_CATEGORY_DESCRIPTION_BYTES: usize = 1_024;
/// Ceiling on how many items one `tag_items` call may move. Bounded so a
/// single model turn cannot rewrite the whole library in one commit.
pub const MAX_CATEGORY_TAG_ITEMS: u16 = 200;
pub const MAX_AGENT_TOOLS_PER_TURN: usize = 53;
pub const MAX_AGENT_MODEL_OUTPUT_BYTES: u64 = 256 * 1_024;
pub const MAX_AGENT_RECALL_EVIDENCE: u16 = 8;
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, uniffi::Enum)]
pub enum AgentAuthority {
    None,
    DurableTurnGrant,
    DurableScopedGrant,
    OneShotApproval,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, uniffi::Enum)]
pub enum AgentApprovalDecision {
    Approve,
    Deny,
    Dismiss,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, uniffi::Enum)]
pub enum AgentToolClass {
    ReadOnly,
    ReversibleWrite,
    ExternalSideEffect,
    DestructiveWrite,
    SecretBearing,
    Publication,
    SessionLocal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, uniffi::Enum)]
pub enum AgentExecutionKind {
    RustCommit,
    RustProjection,
    NativeCapability,
    NativeConversationPresentation,
    NativeCapabilityAndNmpPublication,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, uniffi::Record)]
pub struct AgentToolPolicy {
    pub tool: AgentToolName,
    pub classes: Vec<AgentToolClass>,
    pub authority: AgentAuthority,
    pub execution: AgentExecutionKind,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, uniffi::Enum)]
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
        execute_first: bool,
    },
    QueryTranscripts {
        query: String,
        scope: RecallScope,
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
    /// One upsert-or-delete primitive over a single category rather than a
    /// create/update/delete triplet. Absent `category_id` means create;
    /// `delete` means remove. Absent optional fields are left untouched.
    WriteCategory {
        category_id: Option<CategoryId>,
        name: Option<String>,
        description: Option<String>,
        color_hex: Option<String>,
        delete: bool,
    },
    /// Membership primitive. Podcasts and episodes share one address space
    /// (`LibraryItemId`) so the model does not need a separate verb per item
    /// kind — the kernel resolves what each id refers to.
    TagItems {
        category_id: CategoryId,
        add_item_ids: Vec<LibraryItemId>,
        remove_item_ids: Vec<LibraryItemId>,
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
            Self::QueryTranscripts { .. } => AgentToolName::QueryTranscripts,
            Self::PlayEpisode { .. } => AgentToolName::PlayEpisode,
            Self::SetPlaybackRate { .. } => AgentToolName::SetPlaybackRate,
            Self::SetSleepTimer { .. } => AgentToolName::SetSleepTimer,
            Self::CreateNote { .. } => AgentToolName::CreateNote,
            Self::RecordMemory { .. } => AgentToolName::RecordMemory,
            Self::Ask { .. } => AgentToolName::Ask,
            Self::ScheduleTask { .. } => AgentToolName::ScheduleTask,
            Self::CancelScheduledTask { .. } => AgentToolName::CancelScheduledTask,
            Self::ChangePodcastCategory { .. } => AgentToolName::ChangePodcastCategory,
            Self::WriteCategory { .. } => AgentToolName::WriteCategory,
            Self::TagItems { .. } => AgentToolName::TagItems,
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

pub use crate::agent_turn_contract::{
    AgentApprovalRequest, AgentCapabilityOutcome, AgentCapabilityRequest, AgentCommitReceipt,
    AgentConversationProjection, AgentModelExecutionRequest, AgentProposalProjection,
    AgentTurnProjection, AgentTurnStage,
};
