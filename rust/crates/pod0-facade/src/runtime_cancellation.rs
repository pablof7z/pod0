use pod0_application::{CoreFailureCode, OperationStage};
use pod0_domain::CancellationId;

use crate::runtime_state::{FacadeState, failure};

impl FacadeState {
    pub(super) fn cancel_operation(&mut self, cancellation_id: CancellationId) {
        self.host_requests.cancel(cancellation_id);
        self.host_queue
            .retain(|request| request.cancellation_id != cancellation_id);
        self.pending_feeds.retain(|_, pending| {
            self.operations
                .iter()
                .find(|operation| operation.command_id == pending.command_id)
                .is_none_or(|operation| operation.cancellation_id != cancellation_id)
        });
        self.pending_evidence_indexes
            .retain(|_, pending| pending.cancellation_id != cancellation_id);
        self.pending_recall_cutovers
            .retain(|_, pending| pending.cancellation_id != cancellation_id);
        self.cancel_recall(cancellation_id);
        for operation in &mut self.operations {
            if operation.cancellation_id == cancellation_id && !operation.stage.is_terminal() {
                operation.stage = OperationStage::Cancelled;
                operation.failure = Some(failure(CoreFailureCode::Cancelled));
            }
        }
    }
}
