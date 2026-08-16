use pod0_application::{CoreFailureCode, OperationStage, RecallStage};
use pod0_domain::CancellationId;

use crate::runtime_state::{FacadeState, failure};

impl FacadeState {
    pub(super) fn cancel_recall(&mut self, cancellation_id: CancellationId) {
        let query_ids = self
            .recalls
            .values()
            .filter(|workflow| {
                workflow.cancellation_id == cancellation_id && !workflow.stage.is_terminal()
            })
            .map(|workflow| workflow.query_id)
            .collect::<Vec<_>>();
        for query_id in query_ids {
            let Some(workflow) = self.recalls.get_mut(&query_id) else {
                continue;
            };
            let recall_failure = failure(CoreFailureCode::Cancelled);
            workflow.stage = RecallStage::Cancelled;
            workflow.failure = Some(recall_failure.clone());
            workflow.evidence.clear();
            let command_id = workflow.command_id;
            self.finish(
                command_id,
                OperationStage::Cancelled,
                Some(recall_failure),
                None,
            );
        }
    }
}
