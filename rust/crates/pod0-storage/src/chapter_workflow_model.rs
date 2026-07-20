use pod0_domain::{
    CancellationId, ChapterArtifactId, CommandId, EpisodeId, HostRequestId, StateRevision,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PublisherChapterWorkflowState {
    Requested,
    RetryScheduled,
    Failed,
    Cancelled,
    Succeeded,
    SourceAbsent,
}

impl PublisherChapterWorkflowState {
    pub(crate) const fn wire(self) -> &'static str {
        match self {
            Self::Requested => "requested",
            Self::RetryScheduled => "retry_scheduled",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Succeeded => "succeeded",
            Self::SourceAbsent => "source_absent",
        }
    }

    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "requested" => Some(Self::Requested),
            "retry_scheduled" => Some(Self::RetryScheduled),
            "failed" => Some(Self::Failed),
            "cancelled" => Some(Self::Cancelled),
            "succeeded" => Some(Self::Succeeded),
            "source_absent" => Some(Self::SourceAbsent),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublisherChapterWorkflowRecord {
    pub episode_id: EpisodeId,
    pub source_url: String,
    pub source_version: String,
    pub state: PublisherChapterWorkflowState,
    pub generation: u64,
    pub workflow_revision: StateRevision,
    pub attempt: u16,
    pub max_attempts: u16,
    pub command_id: CommandId,
    pub cancellation_id: CancellationId,
    pub request_id: Option<HostRequestId>,
    pub issued_revision: StateRevision,
    pub expected_selection_revision: StateRevision,
    pub deadline_at_ms: Option<i64>,
    pub not_before_ms: Option<i64>,
    pub selected_artifact_id: Option<ChapterArtifactId>,
    pub failure_code: Option<String>,
    pub failure_detail: Option<String>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublisherChapterWorkflowPage {
    pub items: Vec<PublisherChapterWorkflowRecord>,
    pub has_more: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PublisherChapterEnsureOutcome {
    Requested {
        record: PublisherChapterWorkflowRecord,
        replaced: Option<Box<PublisherChapterWorkflowRecord>>,
    },
    Existing(PublisherChapterWorkflowRecord),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PublisherChapterWorkflowUpdate {
    RetryScheduled(PublisherChapterWorkflowRecord),
    Failed(PublisherChapterWorkflowRecord),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublisherChapterWorkflowFailureInput {
    pub request_id: HostRequestId,
    pub failure_code: String,
    pub failure_detail: Option<String>,
    pub retry_at_ms: Option<i64>,
    pub retry_issued_revision: StateRevision,
    pub retry_deadline_at_ms: Option<i64>,
    pub observed_at_ms: i64,
}
