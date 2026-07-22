use std::sync::{Arc, Mutex, MutexGuard};

use pod0_application::{
    ApplicationCommand, CommandEnvelope, HostCancellationRequest, HostObservationEnvelope,
    HostRequestEnvelope, ProjectionEnvelope, ProjectionRequest, bounded_host_request_count,
};
use pod0_domain::{CancellationId, SubscriptionId};
use pod0_recall_index::{
    RECALL_INDEX_DIMENSIONS, RecallIndex, RecallIndexError, recall_index_path_for_core_store,
};
use pod0_storage::{EvidenceStore, LibraryStore, TranscriptStore};
use std::path::Path;

use crate::runtime_clock::SystemClock;
use crate::runtime_recall_interrupts::RecallInterruptRegistry;
use crate::runtime_state::FacadeState;
use crate::{Pod0ApplicationApi, ProjectionSubscriber};

#[derive(uniffi::Object)]
pub struct Pod0Facade {
    state: Mutex<FacadeState>,
    pub(super) recall_interrupts: Arc<RecallInterruptRegistry>,
}

#[derive(Debug, uniffi::Error)]
pub enum FacadeOpenError {
    NotAuthoritative,
    SchemaBlocked,
    StorageUnavailable,
}

impl std::fmt::Display for FacadeOpenError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::NotAuthoritative => "shared listening store is not authoritative",
            Self::SchemaBlocked => "shared listening store schema is blocked",
            Self::StorageUnavailable => "shared listening store is unavailable",
        })
    }
}

impl std::error::Error for FacadeOpenError {}

impl Pod0Facade {
    #[cfg(test)]
    pub(super) fn with_clock(clock: Arc<dyn pod0_application::Clock>) -> Arc<Self> {
        Self::from_state(FacadeState::with_clock(clock))
    }

    fn from_state(state: FacadeState) -> Arc<Self> {
        let recall_interrupts = Arc::clone(&state.recall_interrupts);
        Arc::new(Self {
            state: Mutex::new(state),
            recall_interrupts,
        })
    }

    pub(super) fn state(&self) -> MutexGuard<'_, FacadeState> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    #[cfg(test)]
    pub(super) fn open_with_clock(
        store_path: String,
        clock: Arc<dyn pod0_application::Clock>,
    ) -> Arc<Self> {
        Self::open_with_clock_value(store_path, clock).expect("test store must open")
    }

    fn open_with_clock_value(
        store_path: String,
        clock: Arc<dyn pod0_application::Clock>,
    ) -> Result<Arc<Self>, FacadeOpenError> {
        let path = Path::new(&store_path);
        let store = LibraryStore::open_authoritative(path).map_err(FacadeOpenError::from)?;
        if !pod0_storage::chapter_store_is_authoritative(path).map_err(FacadeOpenError::from)? {
            return Err(FacadeOpenError::NotAuthoritative);
        }
        store
            .require_notes_authoritative()
            .map_err(FacadeOpenError::from)?;
        let evidence_store = EvidenceStore::open(path).map_err(FacadeOpenError::from)?;
        let transcript_store =
            TranscriptStore::open_authoritative(path).map_err(FacadeOpenError::from)?;
        let scheduled_agent_store = pod0_storage::scheduled_agent_store_is_authoritative(path)
            .map_err(FacadeOpenError::from)?
            .then(|| pod0_storage::ScheduledAgentStore::open_authoritative(path))
            .transpose()
            .map_err(FacadeOpenError::from)?;
        let recall_index = RecallIndex::open(
            &recall_index_path_for_core_store(path),
            RECALL_INDEX_DIMENSIONS,
        )
        .map_err(FacadeOpenError::from)?;
        let state = FacadeState::open(
            store,
            evidence_store,
            transcript_store,
            scheduled_agent_store,
            recall_index,
            clock,
        )
        .map_err(FacadeOpenError::from)?;
        Ok(Self::from_state(state))
    }

    pub(super) fn notify_subscribers(&self) {
        let deliveries = self.state().deliveries();
        for (subscriber, projection) in deliveries {
            subscriber.receive(projection);
        }
    }
}

#[uniffi::export]
impl Pod0Facade {
    #[uniffi::constructor]
    pub fn new() -> Arc<Self> {
        Self::from_state(FacadeState::default())
    }

    #[uniffi::constructor]
    pub fn open(store_path: String) -> Result<Arc<Self>, FacadeOpenError> {
        Self::open_with_clock_value(store_path, Arc::new(SystemClock))
    }

    pub fn dispatch(&self, command: CommandEnvelope) {
        let cancellation_id = cancellation_target(&command);
        if let Some(cancellation_id) = cancellation_id {
            self.recall_interrupts.signal(cancellation_id);
        }
        let changed = self.state().dispatch(command);
        if let Some(cancellation_id) = cancellation_id {
            self.recall_interrupts.finish_signal(cancellation_id);
        }
        if changed {
            self.notify_subscribers();
        }
    }

    pub fn snapshot(&self, request: ProjectionRequest) -> ProjectionEnvelope {
        self.state().snapshot(request)
    }

    /// Plans the exact bounded chapter-model capability request from the
    /// authoritative Rust episode, transcript, and chapter selections.
    pub fn plan_chapter_model_request(
        &self,
        episode_id: pod0_domain::EpisodeId,
        configured_model: String,
    ) -> pod0_application::ChapterModelPlan {
        self.chapter_model_plan(episode_id, configured_model)
    }

    pub fn subscribe(
        &self,
        request: ProjectionRequest,
        subscriber: Arc<dyn ProjectionSubscriber>,
    ) -> SubscriptionId {
        let (subscription_id, projection) = {
            let mut state = self.state();
            let id = state.subscriptions.subscribe(request);
            state.subscribers.insert(id, Arc::clone(&subscriber));
            (id, state.snapshot(request))
        };
        subscriber.receive(projection);
        subscription_id
    }

    pub fn unsubscribe(&self, subscription_id: SubscriptionId) {
        let mut state = self.state();
        let _ = state.subscriptions.unsubscribe(subscription_id);
        state.subscribers.remove(&subscription_id);
    }

    pub fn next_host_requests(&self, maximum_count: u16) -> Vec<HostRequestEnvelope> {
        let (changed, requests) = {
            let mut state = self.state();
            let mut changed = state.retry_pending_publisher_observations();
            changed |= state.reconcile_download_deadlines();
            let _ = state.admit_publisher_chapter_requests();
            let _ = state.admit_download_requests();
            let _ = state.admit_scheduled_agent_requests();
            let maximum = bounded_host_request_count(maximum_count);
            let first_count = maximum.min(state.host_queue.len());
            let mut requests = state.host_queue.drain(..first_count).collect::<Vec<_>>();
            if requests.len() < maximum && state.prepare_model_chapter_host_request() {
                changed = true;
            }
            if requests.len() < maximum && state.prepare_transcript_host_request() {
                changed = true;
            }
            let remaining = maximum
                .saturating_sub(requests.len())
                .min(state.host_queue.len());
            requests.extend(state.host_queue.drain(..remaining));
            (changed, requests)
        };
        if changed {
            self.notify_subscribers();
        }
        requests
    }

    pub fn next_host_cancellations(&self, maximum_count: u16) -> Vec<HostCancellationRequest> {
        let mut state = self.state();
        let count = bounded_host_request_count(maximum_count).min(state.host_cancellations.len());
        state.host_cancellations.drain(..count).collect()
    }

    pub fn record_host_observation(
        &self,
        observation: HostObservationEnvelope,
    ) -> pod0_application::HostObservationReceipt {
        let (changed, receipt) = self.state().record_host_observation(observation);
        if changed {
            self.notify_subscribers();
        }
        receipt
    }
}

fn cancellation_target(command: &CommandEnvelope) -> Option<CancellationId> {
    match command.command {
        ApplicationCommand::CancelOperation { cancellation_id } => Some(cancellation_id),
        _ => None,
    }
}

impl From<pod0_storage::StorageError> for FacadeOpenError {
    fn from(value: pod0_storage::StorageError) -> Self {
        match value {
            pod0_storage::StorageError::CutoverNotAuthoritative
            | pod0_storage::StorageError::ImportNotFound => Self::NotAuthoritative,
            pod0_storage::StorageError::ForeignDatabase
            | pod0_storage::StorageError::CorruptSchema { .. }
            | pod0_storage::StorageError::NewerSchema { .. }
            | pod0_storage::StorageError::FailedMigration { .. }
            | pod0_storage::StorageError::DowngradeForbidden { .. }
            | pod0_storage::StorageError::UnsupportedTarget { .. } => Self::SchemaBlocked,
            _ => Self::StorageUnavailable,
        }
    }
}

impl From<RecallIndexError> for FacadeOpenError {
    fn from(value: RecallIndexError) -> Self {
        match value {
            RecallIndexError::IncompatibleSchema => Self::SchemaBlocked,
            _ => Self::StorageUnavailable,
        }
    }
}

impl Pod0ApplicationApi for Pod0Facade {
    fn dispatch(&self, command: CommandEnvelope) {
        Self::dispatch(self, command);
    }

    fn snapshot(&self, request: ProjectionRequest) -> ProjectionEnvelope {
        Self::snapshot(self, request)
    }

    fn subscribe(
        &self,
        request: ProjectionRequest,
        subscriber: Arc<dyn ProjectionSubscriber>,
    ) -> SubscriptionId {
        Self::subscribe(self, request, subscriber)
    }

    fn unsubscribe(&self, subscription_id: SubscriptionId) {
        Self::unsubscribe(self, subscription_id);
    }

    fn next_host_requests(&self, maximum_count: u16) -> Vec<HostRequestEnvelope> {
        Self::next_host_requests(self, maximum_count)
    }

    fn next_host_cancellations(&self, maximum_count: u16) -> Vec<HostCancellationRequest> {
        Self::next_host_cancellations(self, maximum_count)
    }

    fn record_host_observation(
        &self,
        observation: HostObservationEnvelope,
    ) -> pod0_application::HostObservationReceipt {
        Self::record_host_observation(self, observation)
    }
}
