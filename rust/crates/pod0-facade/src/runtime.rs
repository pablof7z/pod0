use std::sync::{Arc, Mutex, MutexGuard};

use pod0_application::{
    CommandEnvelope, HostObservationEnvelope, HostRequestEnvelope, ProjectionEnvelope,
    ProjectionRequest, bounded_host_request_count,
};
use pod0_domain::SubscriptionId;

use crate::runtime_state::FacadeState;
use crate::{Pod0ApplicationApi, ProjectionSubscriber};

#[derive(uniffi::Object)]
pub struct Pod0Facade {
    state: Mutex<FacadeState>,
}

impl Pod0Facade {
    fn state(&self) -> MutexGuard<'_, FacadeState> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn notify_subscribers(&self) {
        let deliveries = self.state().deliveries();
        for (subscriber, projection) in deliveries {
            let _ = subscriber.receive(projection);
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
        let _ = subscriber.receive(projection);
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
