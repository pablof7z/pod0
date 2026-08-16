use pod0_domain::{
    AgentTurnId, CancellationId, HostRequestId, RecallEmbeddingProvider, RecallQueryId,
    RecallRerankProvider, StateRevision, UnixTimestampMilliseconds,
};

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DurableAgentRecallEffectRequest {
    pub turn_id: AgentTurnId,
    pub request_id: HostRequestId,
    pub cancellation_id: CancellationId,
    pub issued_revision: StateRevision,
    pub deadline_at: UnixTimestampMilliseconds,
    pub query: crate::RecallQuery,
    pub embedding_provider: RecallEmbeddingProvider,
    pub embedding_model: String,
    pub reranker: Option<(RecallRerankProvider, String)>,
    pub phase: AgentRecallEffectPhase,
}

#[must_use]
pub fn agent_recall_request_id(
    turn_id: AgentTurnId,
    query_id: RecallQueryId,
    phase: &AgentRecallEffectPhase,
) -> HostRequestId {
    use sha2::{Digest as _, Sha256};
    let mut hash = Sha256::new();
    hash.update(b"pod0/agent-recall/request/v1");
    hash.update(turn_id.into_bytes());
    hash.update(query_id.into_bytes());
    hash.update(match phase {
        AgentRecallEffectPhase::EmbedQuery => b"embed".as_slice(),
        AgentRecallEffectPhase::Rerank { .. } => b"rerank".as_slice(),
    });
    HostRequestId::from_bytes(hash.finalize()[..16].try_into().expect("digest prefix"))
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum AgentRecallEffectPhase {
    EmbedQuery,
    Rerank {
        candidates: Vec<crate::RecallRerankDocument>,
        evidence: Vec<crate::RecallEvidenceProjection>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DurableAgentRecallHostObservation {
    pub request_id: pod0_domain::HostRequestId,
    pub cancellation_id: pod0_domain::CancellationId,
    pub observed_request_revision: pod0_domain::StateRevision,
    pub sequence_number: u64,
    pub observed_at: pod0_domain::UnixTimestampMilliseconds,
    pub outcome: AgentRecallHostOutcome,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum AgentRecallHostOutcome {
    QueryEmbedded {
        query_id: RecallQueryId,
        embedding: crate::RecallEmbeddingVector,
    },
    CandidatesReranked {
        query_id: RecallQueryId,
        rankings: Vec<crate::RecallRerankObservation>,
    },
    Failed {
        code: crate::HostFailureCode,
        safe_detail: Option<String>,
    },
    Cancelled,
}

impl DurableAgentRecallHostObservation {
    pub fn from_host(value: &crate::HostObservationEnvelope) -> Option<Self> {
        let outcome = match &value.observation {
            crate::HostObservation::RecallQueryEmbedded {
                query_id,
                embedding,
            } => AgentRecallHostOutcome::QueryEmbedded {
                query_id: *query_id,
                embedding: embedding.clone(),
            },
            crate::HostObservation::RecallCandidatesReranked { query_id, rankings } => {
                AgentRecallHostOutcome::CandidatesReranked {
                    query_id: *query_id,
                    rankings: rankings.clone(),
                }
            }
            crate::HostObservation::Failed { code, safe_detail } => {
                AgentRecallHostOutcome::Failed {
                    code: *code,
                    safe_detail: safe_detail.clone(),
                }
            }
            crate::HostObservation::Cancelled => AgentRecallHostOutcome::Cancelled,
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
