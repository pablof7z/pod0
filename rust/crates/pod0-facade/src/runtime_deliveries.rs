use std::sync::Arc;

use pod0_application::ProjectionEnvelope;

use crate::ProjectionSubscriber;
use crate::runtime_state::FacadeState;

impl FacadeState {
    pub(super) fn deliveries(
        &mut self,
    ) -> Vec<(Arc<dyn ProjectionSubscriber>, ProjectionEnvelope)> {
        let subscription_ids = self.subscribers.keys().copied().collect::<Vec<_>>();
        let mut deliveries = Vec::new();
        for id in subscription_ids {
            let Some(request) = self.subscriptions.request(id) else {
                continue;
            };
            let Some(subscriber) = self.subscribers.get(&id).cloned() else {
                continue;
            };
            let mut envelope = self.snapshot(request);
            let content = self.delivery_content(request, &envelope.projection);
            let content_changed = self.delivered_contents.get(&id) != Some(&content);
            let operations_changed =
                self.delivered_projections.get(&id) != Some(&envelope.projection);
            let deliver_operations =
                matches!(request.scope, pod0_application::ProjectionScope::Library)
                    && operations_changed;
            if !content_changed && !deliver_operations {
                continue;
            }
            envelope.content_changed = content_changed;
            self.delivered_contents.insert(id, content);
            self.delivered_projections
                .insert(id, envelope.projection.clone());
            deliveries.push((subscriber, envelope));
        }
        deliveries
    }
}
