use std::sync::{Arc, Mutex, MutexGuard};

use pod0_application::{
    CommandEnvelope, HostObservationEnvelope, HostRequestEnvelope, ProjectionEnvelope,
    ProjectionRequest, bounded_host_request_count,
};
use pod0_domain::SubscriptionId;
use pod0_storage::{EvidenceStore, LibraryStore};
use std::path::Path;

use crate::runtime_state::FacadeState;
use crate::{Pod0ApplicationApi, ProjectionSubscriber};

#[derive(uniffi::Object)]
pub struct Pod0Facade {
    state: Mutex<FacadeState>,
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
        Arc::new(Self {
            state: Mutex::new(FacadeState::with_clock(clock)),
        })
    }

    fn state(&self) -> MutexGuard<'_, FacadeState> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn notify_subscribers(&self) {
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
        Arc::new(Self {
            state: Mutex::new(FacadeState::default()),
        })
    }

    #[uniffi::constructor]
    pub fn open(store_path: String) -> Result<Arc<Self>, FacadeOpenError> {
        let path = Path::new(&store_path);
        let store = LibraryStore::open_authoritative(path).map_err(FacadeOpenError::from)?;
        let evidence_store = EvidenceStore::open(path).map_err(FacadeOpenError::from)?;
        let state = FacadeState::open(store, evidence_store).map_err(FacadeOpenError::from)?;
        Ok(Arc::new(Self {
            state: Mutex::new(state),
        }))
    }

    pub fn dispatch(&self, command: CommandEnvelope) {
        if self.state().dispatch(command) {
            self.notify_subscribers();
        }
    }

    pub fn snapshot(&self, request: ProjectionRequest) -> ProjectionEnvelope {
        self.state().snapshot(request)
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
        let mut state = self.state();
        let request_count = bounded_host_request_count(maximum_count).min(state.host_queue.len());
        state.host_queue.drain(..request_count).collect()
    }

    pub fn record_host_observation(&self, observation: HostObservationEnvelope) {
        if self.state().record_host_observation(observation) {
            self.notify_subscribers();
        }
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

    fn record_host_observation(&self, observation: HostObservationEnvelope) {
        Self::record_host_observation(self, observation);
    }
}
