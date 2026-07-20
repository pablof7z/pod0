use pod0_domain::{CancellationId, CommandId, EpisodeId, EvidenceGenerationId, EvidenceSpanId};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PendingEvidenceIndex {
    pub(super) command_id: CommandId,
    pub(super) cancellation_id: CancellationId,
    pub(super) episode_id: EpisodeId,
    pub(super) generation_id: EvidenceGenerationId,
    pub(super) expected_span_count: u32,
    pub(super) requested_span_ids: Vec<EvidenceSpanId>,
}
