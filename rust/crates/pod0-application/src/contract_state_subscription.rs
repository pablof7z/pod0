use std::collections::BTreeMap;

use pod0_domain::SubscriptionId;

use crate::ProjectionRequest;

#[derive(Default)]
pub struct SubscriptionRegistry {
    next_value: u64,
    subscriptions: BTreeMap<SubscriptionId, ProjectionRequest>,
}

impl SubscriptionRegistry {
    #[must_use]
    pub fn subscribe(&mut self, request: ProjectionRequest) -> SubscriptionId {
        self.next_value = self
            .next_value
            .checked_add(1)
            .expect("subscription ID exhausted");
        let id = SubscriptionId::from_parts(0, self.next_value);
        self.subscriptions.insert(id, request);
        id
    }

    #[must_use]
    pub fn unsubscribe(&mut self, subscription_id: SubscriptionId) -> bool {
        self.subscriptions.remove(&subscription_id).is_some()
    }

    #[must_use]
    pub fn request(&self, subscription_id: SubscriptionId) -> Option<ProjectionRequest> {
        self.subscriptions.get(&subscription_id).copied()
    }
}
