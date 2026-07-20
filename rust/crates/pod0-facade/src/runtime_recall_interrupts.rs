use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use pod0_domain::CancellationId;
use pod0_recall_index::RecallCancellation;

#[derive(Default)]
pub(super) struct RecallInterruptRegistry {
    state: Mutex<RecallInterruptState>,
}

#[derive(Default)]
struct RecallInterruptState {
    active: BTreeMap<CancellationId, RecallCancellation>,
    cancelled: BTreeSet<CancellationId>,
}

impl RecallInterruptRegistry {
    pub(super) fn begin(
        self: &Arc<Self>,
        cancellation_id: CancellationId,
        cancellation: RecallCancellation,
    ) -> RecallInterruptLease {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.cancelled.contains(&cancellation_id) {
            cancellation.cancel();
        }
        state.active.insert(cancellation_id, cancellation.clone());
        RecallInterruptLease {
            registry: Arc::clone(self),
            cancellation_id,
            cancellation,
        }
    }

    pub(super) fn signal(&self, cancellation_id: CancellationId) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.cancelled.insert(cancellation_id);
        if let Some(cancellation) = state.active.get(&cancellation_id) {
            cancellation.cancel();
        }
    }

    pub(super) fn finish_signal(&self, cancellation_id: CancellationId) {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .cancelled
            .remove(&cancellation_id);
    }

    fn finish_operation(&self, cancellation_id: CancellationId) {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .active
            .remove(&cancellation_id);
    }
}

pub(super) struct RecallInterruptLease {
    registry: Arc<RecallInterruptRegistry>,
    cancellation_id: CancellationId,
    cancellation: RecallCancellation,
}

impl RecallInterruptLease {
    pub(super) fn cancellation(&self) -> &RecallCancellation {
        &self.cancellation
    }
}

impl Drop for RecallInterruptLease {
    fn drop(&mut self) {
        self.registry.finish_operation(self.cancellation_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancellation_before_registration_is_not_lost() {
        let registry = Arc::new(RecallInterruptRegistry::default());
        let cancellation_id = CancellationId::from_parts(1, 2);
        registry.signal(cancellation_id);

        let lease = registry.begin(cancellation_id, RecallCancellation::default());

        assert!(lease.cancellation().is_cancelled());
        registry.finish_signal(cancellation_id);
    }
}
