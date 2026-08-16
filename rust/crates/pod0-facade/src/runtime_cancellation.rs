use pod0_application::{CommandEnvelope, CoreFailureCode, OperationStage};
use pod0_domain::{CancellationId, ContentDigest};

use crate::runtime_state::{FacadeState, failure};

impl FacadeState {
    pub(super) fn cancel_operation_with_activity(
        &mut self,
        envelope: &CommandEnvelope,
        fingerprint: ContentDigest,
        cancellation_id: CancellationId,
    ) -> bool {
        let cancellation = self.store.as_ref().map_or(
            Err(pod0_storage::StorageError::CutoverNotAuthoritative),
            |store| {
                store.cancel_durable_effects(
                    envelope.command_id,
                    fingerprint,
                    cancellation_id,
                    self.now(),
                )
            },
        );
        if cancellation.is_err() {
            self.fail(envelope.command_id, CoreFailureCode::StorageUnavailable);
            return false;
        }
        self.cancel_operation(cancellation_id);
        true
    }

    pub(super) fn cancel_operation(&mut self, cancellation_id: CancellationId) {
        self.host_requests.cancel(cancellation_id);
        if !self.feed_fetches.is_empty() {
            let _ = self.reload_feed_fetches();
        }
        self.cancel_recall(cancellation_id);
        for operation in &mut self.operations {
            if operation.cancellation_id == cancellation_id && !operation.stage.is_terminal() {
                operation.stage = OperationStage::Cancelled;
                operation.failure = Some(failure(CoreFailureCode::Cancelled));
            }
        }
    }
}
