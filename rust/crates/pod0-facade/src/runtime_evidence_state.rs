use pod0_domain::{
    CancellationId, CommandId, EpisodeId, EvidenceGenerationId, EvidenceSpanId, StateRevision,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct EvidenceIndexTarget {
    pub(super) episode_id: EpisodeId,
    pub(super) generation_id: EvidenceGenerationId,
    pub(super) expected_span_count: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum EvidenceIndexCompletion {
    EvidenceRebuild,
    TranscriptWorkflow {
        workflow_id: pod0_domain::TranscriptWorkflowId,
        input_version: String,
    },
    RecallConfiguration {
        imported: Option<bool>,
        revision: StateRevision,
        completed_episode_count: u32,
        remaining: Vec<EvidenceIndexTarget>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PendingEvidenceIndex {
    pub(super) command_id: CommandId,
    pub(super) cancellation_id: CancellationId,
    pub(super) episode_id: EpisodeId,
    pub(super) generation_id: EvidenceGenerationId,
    pub(super) expected_span_count: u32,
    pub(super) requested_span_ids: Vec<EvidenceSpanId>,
    pub(super) completion: EvidenceIndexCompletion,
}
