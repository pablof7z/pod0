use pod0_domain::{CancellationId, CommandId, EpisodeId, PodcastId, StateRevision};

pub const FACADE_CONTRACT_VERSION: u32 = 1;
pub const MAX_PROJECTION_ITEMS: u16 = 200;
pub const MAX_OPERATION_ITEMS: usize = 32;
pub const MAX_HOST_REQUEST_BATCH: u16 = 64;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandEnvelope {
    pub command_id: CommandId,
    pub cancellation_id: CancellationId,
    pub expected_revision: Option<StateRevision>,
    pub command: ApplicationCommand,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ApplicationCommand {
    SubscribeToFeed { feed_url: String },
    Unsubscribe { podcast_id: PodcastId },
    RequestPlayback { episode_id: EpisodeId },
    CancelOperation { cancellation_id: CancellationId },
    Unsupported { wire_code: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectionScope {
    Library,
    Playback,
    Unsupported { wire_code: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProjectionRequest {
    pub scope: ProjectionScope,
    pub max_items: u16,
}

impl ProjectionRequest {
    #[must_use]
    pub fn bounded_max_items(self) -> usize {
        usize::from(self.max_items.clamp(1, MAX_PROJECTION_ITEMS))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectionEnvelope {
    pub contract_version: u32,
    pub state_revision: StateRevision,
    pub projection: Projection,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Projection {
    Library(LibraryProjection),
    Playback(PlaybackProjection),
    Unsupported(UnsupportedProjection),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnsupportedProjection {
    pub wire_code: u32,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LibraryProjection {
    pub podcasts: Vec<PodcastSummary>,
    pub episodes: Vec<EpisodeSummary>,
    pub operations: Vec<OperationProjection>,
    pub has_more: bool,
}

impl LibraryProjection {
    pub fn enforce_bounds(&mut self, requested_items: usize) {
        let item_limit = requested_items.clamp(1, usize::from(MAX_PROJECTION_ITEMS));
        let was_truncated = self.podcasts.len() > item_limit || self.episodes.len() > item_limit;
        self.podcasts.truncate(item_limit);
        self.episodes.truncate(item_limit);
        self.operations.truncate(MAX_OPERATION_ITEMS);
        self.has_more |= was_truncated;
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PodcastSummary {
    pub podcast_id: PodcastId,
    pub title: String,
    pub subscribed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EpisodeSummary {
    pub episode_id: EpisodeId,
    pub podcast_id: PodcastId,
    pub title: String,
    pub duration_milliseconds: Option<u64>,
    pub resume_position_milliseconds: u64,
    pub completed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlaybackProjection {
    pub current: Option<PlaybackItem>,
    pub queue: Vec<EpisodeId>,
    pub operations: Vec<OperationProjection>,
}

impl PlaybackProjection {
    pub fn enforce_bounds(&mut self, requested_items: usize) {
        let item_limit = requested_items.clamp(1, usize::from(MAX_PROJECTION_ITEMS));
        self.queue.truncate(item_limit);
        self.operations.truncate(MAX_OPERATION_ITEMS);
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlaybackItem {
    pub episode_id: EpisodeId,
    pub title: String,
    pub durable_resume_position_milliseconds: u64,
    pub policy_state: PlaybackPolicyState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaybackPolicyState {
    Idle,
    AwaitingHost,
    Playing,
    Paused,
    Completed,
    Unsupported { wire_code: u32 },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OperationProjection {
    pub command_id: CommandId,
    pub cancellation_id: CancellationId,
    pub stage: OperationStage,
    pub failure: Option<CoreFailure>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OperationStage {
    Accepted,
    Running,
    Blocked,
    Failed,
    Cancelled,
    Succeeded,
    Unsupported { wire_code: u32 },
}

impl OperationStage {
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Failed | Self::Cancelled | Self::Succeeded | Self::Unsupported { .. }
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreFailure {
    pub code: CoreFailureCode,
    pub safe_detail: Option<String>,
    pub retryability: Retryability,
    pub user_action: UserAction,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoreFailureCode {
    InvalidCommand,
    RevisionConflict,
    NotFound,
    HostUnavailable,
    HostRejected,
    Cancelled,
    Unsupported { wire_code: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Retryability {
    Never,
    Automatic,
    AfterUserAction,
    Unsupported { wire_code: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UserAction {
    None,
    Retry,
    CheckConnection,
    ReviewPermissions,
    Unsupported { wire_code: u32 },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projection_requests_are_always_bounded() {
        let empty = ProjectionRequest {
            scope: ProjectionScope::Library,
            max_items: 0,
        };
        let oversized = ProjectionRequest {
            scope: ProjectionScope::Playback,
            max_items: u16::MAX,
        };

        assert_eq!(empty.bounded_max_items(), 1);
        assert_eq!(
            oversized.bounded_max_items(),
            usize::from(MAX_PROJECTION_ITEMS)
        );
    }

    #[test]
    fn terminal_or_unknown_stages_clear_busy_state() {
        assert!(!OperationStage::Accepted.is_terminal());
        assert!(!OperationStage::Blocked.is_terminal());
        assert!(OperationStage::Failed.is_terminal());
        assert!(OperationStage::Cancelled.is_terminal());
        assert!(OperationStage::Succeeded.is_terminal());
        assert!(OperationStage::Unsupported { wire_code: 99 }.is_terminal());
    }

    #[test]
    fn projection_construction_enforces_collection_limits() {
        let podcast = PodcastSummary {
            podcast_id: PodcastId::from_parts(0, 1),
            title: "Podcast".to_owned(),
            subscribed: true,
        };
        let operation = OperationProjection {
            command_id: CommandId::from_parts(0, 1),
            cancellation_id: CancellationId::from_parts(0, 1),
            stage: OperationStage::Unsupported { wire_code: 77 },
            failure: None,
        };
        let mut projection = LibraryProjection {
            podcasts: vec![podcast; 4],
            episodes: Vec::new(),
            operations: vec![operation; MAX_OPERATION_ITEMS + 1],
            has_more: false,
        };

        projection.enforce_bounds(2);

        assert_eq!(projection.podcasts.len(), 2);
        assert_eq!(projection.operations.len(), MAX_OPERATION_ITEMS);
        assert!(projection.has_more);
    }
}
