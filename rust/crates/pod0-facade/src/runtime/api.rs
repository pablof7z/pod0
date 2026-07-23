use std::sync::Arc;

use pod0_application::{
    CommandEnvelope, HostCancellationRequest, HostObservationEnvelope, HostObservationReceipt,
    HostRequestEnvelope, ProjectionEnvelope, ProjectionRequest,
};
use pod0_domain::SubscriptionId;

use super::Pod0Facade;
use crate::{Pod0ApplicationApi, ProjectionSubscriber};

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
    ) -> HostObservationReceipt {
        Self::record_host_observation(self, observation)
    }
}
