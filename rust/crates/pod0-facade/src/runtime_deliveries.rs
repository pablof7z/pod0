use std::sync::Arc;

use pod0_application::ProjectionEnvelope;

use crate::ProjectionSubscriber;
use crate::runtime_state::FacadeState;

impl FacadeState {
    pub(super) fn deliveries(&self) -> Vec<(Arc<dyn ProjectionSubscriber>, ProjectionEnvelope)> {
        self.subscribers
            .iter()
            .filter_map(|(id, subscriber)| {
                self.subscriptions
                    .request(*id)
                    .map(|request| (Arc::clone(subscriber), self.snapshot(request)))
            })
            .collect()
    }
}
