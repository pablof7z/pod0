use pod0_application::{CoreFailure, RecallEvidenceProjection, RecallScope, RecallStage};
use pod0_domain::{CancellationId, CommandId, RecallQueryId};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RecallHostPhase {
    Embedding,
    Reranking,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PendingRecall {
    pub(super) query_id: RecallQueryId,
    pub(super) cancellation_id: CancellationId,
    pub(super) phase: RecallHostPhase,
}

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

impl RecallWorkflow {
    #[must_use]
    pub(super) fn new(
        command_id: CommandId,
        cancellation_id: CancellationId,
        query_id: RecallQueryId,
        scope: RecallScope,
        normalized_text: String,
        limit: u16,
    ) -> Self {
        Self {
            command_id,
            cancellation_id,
            query_id,
            scope,
            normalized_text,
            limit,
            stage: RecallStage::Queued,
            failure: None,
            evidence: Vec::new(),
        }
    }
}
