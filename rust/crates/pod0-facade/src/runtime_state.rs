use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;

use pod0_application::{
    Clock, CommandEnvelope, CommandLedger, CommandRegistration, CoreFailure, CoreFailureCode,
    HostRequestEnvelope, HostRequestLedger, OperationProjection, OperationResult, OperationStage,
    Retryability, SubscriptionRegistry, UserAction,
};
use pod0_domain::{
    CommandId, FeedIdentityV1, ListeningDomainSnapshot, PodcastId, StateRevision, SubscriptionId,
};
use pod0_storage::LibraryStore;

use crate::ProjectionSubscriber;
use crate::runtime_clock::SystemClock;

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

pub(super) struct FacadeState {
    clock: Arc<dyn Clock>,
    pub(super) revision: StateRevision,
    pub(super) listening: ListeningDomainSnapshot,
    pub(super) store: Option<LibraryStore>,
    pub(super) commands: CommandLedger,
    pub(super) host_requests: HostRequestLedger,
    pub(super) host_queue: VecDeque<HostRequestEnvelope>,
    pub(super) pending_feeds: BTreeMap<pod0_domain::HostRequestId, PendingFeed>,
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
            commands: CommandLedger::default(),
            host_requests: HostRequestLedger::default(),
            host_queue: VecDeque::new(),
            pending_feeds: BTreeMap::new(),
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

    pub(super) fn open(store: LibraryStore) -> Result<Self, pod0_storage::StorageError> {
        let listening = store.snapshot()?;
        Ok(Self {
            revision: listening.playback.revision,
            listening,
            store: Some(store),
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
            self.listening = store.snapshot()?;
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
            queue: Vec::new(),
            rate: PlaybackRatePermille { value: 1_000 },
            sleep_mode: PlaybackSleepMode::Off,
            auto_mark_played_at_natural_end: true,
            auto_play_next: true,
            revision: StateRevision::INITIAL,
        },
    }
}
