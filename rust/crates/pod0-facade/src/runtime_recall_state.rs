use pod0_application::{CoreFailure, RecallEvidenceProjection, RecallScope, RecallStage};
use pod0_domain::{CancellationId, CommandId, RecallQueryId};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct RecallWorkflow {
    pub(super) command_id: CommandId,
    pub(super) cancellation_id: CancellationId,
    pub(super) query_id: RecallQueryId,
    pub(super) scope: RecallScope,
    pub(super) normalized_text: String,
    pub(super) limit: u16,
    pub(super) stage: RecallStage,
    pub(super) failure: Option<CoreFailure>,
    pub(super) evidence: Vec<RecallEvidenceProjection>,
}
