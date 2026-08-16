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

impl EvidenceIndexCompletion {
    pub(super) fn durable(&self) -> pod0_application::DurableEvidenceIndexCompletion {
        match self {
            Self::EvidenceRebuild => {
                pod0_application::DurableEvidenceIndexCompletion::EvidenceRebuild
            }
            Self::TranscriptWorkflow {
                workflow_id,
                input_version,
            } => pod0_application::DurableEvidenceIndexCompletion::TranscriptWorkflow {
                workflow_id: *workflow_id,
                input_version: input_version.clone(),
            },
            Self::RecallConfiguration {
                imported,
                revision,
                completed_episode_count,
                remaining,
            } => pod0_application::DurableEvidenceIndexCompletion::RecallConfiguration {
                imported: *imported,
                revision: *revision,
                completed_episode_count: *completed_episode_count,
                remaining: remaining
                    .iter()
                    .map(|target| pod0_application::DurableEvidenceIndexTarget {
                        episode_id: target.episode_id,
                        generation_id: target.generation_id,
                        expected_span_count: target.expected_span_count,
                    })
                    .collect(),
            },
        }
    }

    pub(super) fn from_durable(value: pod0_application::DurableEvidenceIndexCompletion) -> Self {
        match value {
            pod0_application::DurableEvidenceIndexCompletion::EvidenceRebuild => {
                Self::EvidenceRebuild
            }
            pod0_application::DurableEvidenceIndexCompletion::TranscriptWorkflow {
                workflow_id,
                input_version,
            } => Self::TranscriptWorkflow {
                workflow_id,
                input_version,
            },
            pod0_application::DurableEvidenceIndexCompletion::RecallConfiguration {
                imported,
                revision,
                completed_episode_count,
                remaining,
            } => Self::RecallConfiguration {
                imported,
                revision,
                completed_episode_count,
                remaining: remaining
                    .into_iter()
                    .map(|target| EvidenceIndexTarget {
                        episode_id: target.episode_id,
                        generation_id: target.generation_id,
                        expected_span_count: target.expected_span_count,
                    })
                    .collect(),
            },
        }
    }
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
