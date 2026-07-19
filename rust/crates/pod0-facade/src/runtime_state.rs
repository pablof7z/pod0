use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;

use pod0_application::{
    Clock, CommandEnvelope, CommandLedger, CommandRegistration, CoreFailure, CoreFailureCode,
    HostRequestEnvelope, HostRequestLedger, OperationProjection, OperationResult, OperationStage,
    PlaybackHostState, PlaybackLifecycleObservation, PlaybackPolicyState, Retryability,
    SubscriptionRegistry, UserAction,
};
use pod0_domain::{
    CommandId, EpisodeId, FeedIdentityV1, HostRequestId, ListeningDomainSnapshot, PodcastId,
    RecallQueryId, StateRevision, SubscriptionId,
};
use pod0_storage::{EvidenceStore, LibraryStore};

use crate::ProjectionSubscriber;
use crate::runtime_clock::SystemClock;
use crate::runtime_recall_state::{PendingRecall, RecallWorkflow};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FeedIntent {
    Subscribe,
    Ensure,
    Refresh,
    Metadata,
}

#[derive(Clone, Debug)]
pub(super) struct PendingFeed {
    pub command_id: CommandId,
    pub fingerprint: String,
    pub intent: FeedIntent,
    pub feed_identity: FeedIdentityV1,
    pub podcast_id: PodcastId,
}

#[derive(Clone, Debug)]
pub(super) struct PlaybackRuntime {
    pub(super) policy_state: PlaybackPolicyState,
    pub(super) host_state: PlaybackHostState,
    pub(super) desired_playing: bool,
    pub(super) media_episode_id: Option<EpisodeId>,
    pub(super) interrupted_episode_id: Option<EpisodeId>,
    pub(super) observation_request_id: Option<HostRequestId>,
    pub(super) last_observation: Option<PlaybackLifecycleObservation>,
    pub(super) last_position_commit_at_ms: Option<i64>,
    pub(super) position_command_fence_at_ms: Option<i64>,
    pub(super) timer_fired: bool,
}

impl Default for PlaybackRuntime {
    fn default() -> Self {
        Self {
            policy_state: PlaybackPolicyState::Idle,
            host_state: PlaybackHostState::Idle,
            desired_playing: false,
            media_episode_id: None,
            interrupted_episode_id: None,
            observation_request_id: None,
            last_observation: None,
            last_position_commit_at_ms: None,
            position_command_fence_at_ms: None,
            timer_fired: false,
        }
    }
}

pub(super) struct FacadeState {
    clock: Arc<dyn Clock>,
    pub(super) revision: StateRevision,
    pub(super) listening: ListeningDomainSnapshot,
    pub(super) store: Option<LibraryStore>,
    pub(super) evidence_store: Option<EvidenceStore>,
    pub(super) commands: CommandLedger,
    pub(super) host_requests: HostRequestLedger,
    pub(super) host_queue: VecDeque<HostRequestEnvelope>,
    pub(super) pending_feeds: BTreeMap<pod0_domain::HostRequestId, PendingFeed>,
    pub(super) pending_recalls: BTreeMap<HostRequestId, PendingRecall>,
    pub(super) recalls: BTreeMap<RecallQueryId, RecallWorkflow>,
    pub(super) playback: PlaybackRuntime,
    pub(super) operations: Vec<OperationProjection>,
    pub(super) subscriptions: SubscriptionRegistry,
    pub(super) subscribers: BTreeMap<SubscriptionId, Arc<dyn ProjectionSubscriber>>,
}

impl Default for FacadeState {
    fn default() -> Self {
        Self {
            clock: Arc::new(SystemClock),
            revision: StateRevision::INITIAL,
            listening: empty_listening_snapshot(),
            store: None,
            evidence_store: None,
            commands: CommandLedger::default(),
            host_requests: HostRequestLedger::default(),
            host_queue: VecDeque::new(),
            pending_feeds: BTreeMap::new(),
            pending_recalls: BTreeMap::new(),
            recalls: BTreeMap::new(),
            playback: PlaybackRuntime::default(),
            operations: Vec::new(),
            subscriptions: SubscriptionRegistry::default(),
            subscribers: BTreeMap::new(),
        }
    }
}

impl FacadeState {
    #[cfg(test)]
    pub(super) fn with_clock(clock: Arc<dyn Clock>) -> Self {
        Self {
            clock,
            ..Self::default()
        }
    }

    pub(super) fn now(&self) -> pod0_domain::UnixTimestampMilliseconds {
        self.clock.now()
    }

    pub(super) fn open(
        store: LibraryStore,
        evidence_store: EvidenceStore,
    ) -> Result<Self, pod0_storage::StorageError> {
        let _ = store.clear_session_sleep_timer()?;
        let listening = store.snapshot()?;
        let playback = PlaybackRuntime {
            policy_state: if listening.playback.active_episode_id.is_some() {
                PlaybackPolicyState::Paused
            } else {
                PlaybackPolicyState::Idle
            },
            ..PlaybackRuntime::default()
        };
        Ok(Self {
            revision: listening.playback.revision,
            listening,
            store: Some(store),
            evidence_store: Some(evidence_store),
            playback,
            ..Self::default()
        })
    }

    pub(super) fn dispatch(&mut self, envelope: CommandEnvelope) -> bool {
        match self.commands.register(envelope.clone(), self.revision) {
            CommandRegistration::Accepted => self.accept_command(envelope),
            CommandRegistration::StaleRevision => {
                self.advance_revision();
                self.operations.push(OperationProjection {
                    command_id: envelope.command_id,
                    cancellation_id: envelope.cancellation_id,
                    stage: OperationStage::Failed,
                    failure: Some(failure(CoreFailureCode::RevisionConflict)),
                    result: None,
                });
                self.trim_operations();
                true
            }
            CommandRegistration::Duplicate | CommandRegistration::ConflictingReuse => false,
        }
    }

    pub(super) fn advance_revision(&mut self) {
        self.revision = StateRevision::new(
            self.revision
                .value
                .checked_add(1)
                .expect("state revision exhausted"),
        );
    }

    pub(super) fn begin(&mut self, envelope: &CommandEnvelope) {
        self.advance_revision();
        self.operations.push(OperationProjection {
            command_id: envelope.command_id,
            cancellation_id: envelope.cancellation_id,
            stage: OperationStage::Accepted,
            failure: None,
            result: None,
        });
    }

    pub(super) fn fail(&mut self, command_id: CommandId, code: CoreFailureCode) {
        self.finish(
            command_id,
            OperationStage::Failed,
            Some(failure(code)),
            None,
        );
    }

    pub(super) fn succeed(&mut self, command_id: CommandId, result: Option<OperationResult>) {
        self.finish(command_id, OperationStage::Succeeded, None, result);
    }

    pub(super) fn finish(
        &mut self,
        command_id: CommandId,
        stage: OperationStage,
        operation_failure: Option<CoreFailure>,
        result: Option<OperationResult>,
    ) {
        if let Some(operation) = self
            .operations
            .iter_mut()
            .rev()
            .find(|operation| operation.command_id == command_id)
        {
            operation.stage = stage;
            operation.failure = operation_failure;
            operation.result = result;
        }
    }

    pub(super) fn reload_listening(&mut self) -> Result<(), pod0_storage::StorageError> {
        if let Some(store) = &self.store {
            let listening = store.snapshot()?;
            self.revision =
                StateRevision::new(self.revision.value.max(listening.playback.revision.value));
            self.listening = listening;
        }
        Ok(())
    }

    pub(super) fn trim_operations(&mut self) {
        if self.operations.len() > pod0_application::MAX_OPERATION_ITEMS {
            let excess = self.operations.len() - pod0_application::MAX_OPERATION_ITEMS;
            self.operations.drain(..excess);
        }
    }
}

pub(super) fn failure(code: CoreFailureCode) -> CoreFailure {
    let (retryability, user_action) = match code {
        CoreFailureCode::HostUnavailable | CoreFailureCode::StorageUnavailable => {
            (Retryability::Automatic, UserAction::Retry)
        }
        CoreFailureCode::InvalidFeedUrl | CoreFailureCode::FeedMalformed => {
            (Retryability::AfterUserAction, UserAction::ReviewPermissions)
        }
        _ => (Retryability::Never, UserAction::None),
    };
    CoreFailure {
        code,
        safe_detail: None,
        retryability,
        user_action,
    }
}

fn empty_listening_snapshot() -> ListeningDomainSnapshot {
    use pod0_domain::{ListeningPlaybackPolicy, PlaybackRatePermille, PlaybackSleepMode};
    ListeningDomainSnapshot {
        podcasts: Vec::new(),
        subscriptions: Vec::new(),
        episodes: Vec::new(),
        playback: ListeningPlaybackPolicy {
            active_episode_id: None,
            active_segment: None,
            active_label: None,
            queue: Vec::new(),
            rate: PlaybackRatePermille { value: 1_000 },
            sleep_mode: PlaybackSleepMode::Off,
            auto_mark_played_at_natural_end: true,
            auto_play_next: true,
            revision: StateRevision::INITIAL,
        },
    }
}
