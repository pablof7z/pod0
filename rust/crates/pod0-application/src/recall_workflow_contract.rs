use pod0_domain::{
    CancellationId, CommandId, RecallQueryId, StateRevision, UnixTimestampMilliseconds,
};

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DurableRecallQueryEffectRequest {
    pub command_id: CommandId,
    pub cancellation_id: CancellationId,
    pub request_id: pod0_domain::HostRequestId,
    pub issued_revision: StateRevision,
    pub deadline_at: UnixTimestampMilliseconds,
    pub query: crate::RecallQuery,
    pub embedding_provider: pod0_domain::RecallEmbeddingProvider,
    pub embedding_model: String,
    pub reranker: Option<(pod0_domain::RecallRerankProvider, String)>,
    pub phase: crate::AgentRecallEffectPhase,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DurableRecallIndexCutoverEffectRequest {
    pub command_id: CommandId,
    pub cancellation_id: CancellationId,
    pub request_id: pod0_domain::HostRequestId,
    pub issued_revision: StateRevision,
    pub deadline_at: UnixTimestampMilliseconds,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DurableRecallIndexCutoverHostObservation {
    pub request_id: pod0_domain::HostRequestId,
    pub cancellation_id: CancellationId,
    pub observed_request_revision: StateRevision,
    pub sequence_number: u64,
    pub observed_at: UnixTimestampMilliseconds,
    pub outcome: RecallIndexCutoverHostOutcome,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum RecallIndexCutoverHostOutcome {
    ArtifactsRemoved {
        removed_file_count: u32,
    },
    Failed {
        code: crate::HostFailureCode,
        safe_detail: Option<String>,
    },
    Cancelled,
}

impl DurableRecallIndexCutoverHostObservation {
    pub fn from_host(value: &crate::HostObservationEnvelope) -> Option<Self> {
        let outcome = match &value.observation {
            crate::HostObservation::LegacyRecallIndexArtifactsRemoved { removed_file_count } => {
                RecallIndexCutoverHostOutcome::ArtifactsRemoved {
                    removed_file_count: u32::from(*removed_file_count),
                }
            }
            crate::HostObservation::Failed { code, safe_detail } => {
                RecallIndexCutoverHostOutcome::Failed {
                    code: *code,
                    safe_detail: safe_detail.clone(),
                }
            }
            crate::HostObservation::Cancelled => RecallIndexCutoverHostOutcome::Cancelled,
            _ => return None,
        };
        Some(Self {
            request_id: value.request_id,
            cancellation_id: value.cancellation_id,
            observed_request_revision: value.observed_request_revision,
            sequence_number: value.sequence_number,
            observed_at: value.observed_at,
            outcome,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct StoredRecallQueryWorkflow {
    pub command_id: CommandId,
    pub cancellation_id: CancellationId,
    pub query: crate::RecallQuery,
    pub revision: StateRevision,
    pub stage: crate::RecallStage,
    pub evidence: Vec<crate::RecallEvidenceProjection>,
    pub failure: Option<crate::CoreFailure>,
    pub created_at: UnixTimestampMilliseconds,
    pub updated_at: UnixTimestampMilliseconds,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum RecallQueryResolution {
    Rerank {
        request: DurableRecallQueryEffectRequest,
    },
    Finish {
        stage: crate::RecallStage,
        evidence: Vec<crate::RecallEvidenceProjection>,
        failure: Option<crate::CoreFailure>,
    },
}

#[must_use]
pub fn recall_query_request_id(
    query_id: RecallQueryId,
    phase: &crate::AgentRecallEffectPhase,
) -> pod0_domain::HostRequestId {
    crate::agent_recall_request_id(
        pod0_domain::AgentTurnId::from_bytes(query_id.into_bytes()),
        query_id,
        phase,
    )
}
