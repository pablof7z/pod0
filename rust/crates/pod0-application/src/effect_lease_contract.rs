use pod0_domain::{
    ActivityCorrelationId, ActivityId, EffectAttemptId, EffectIntentId, EffectLeaseId,
    UnixTimestampMilliseconds,
};

use crate::{
    HostObservation, HostObservationEnvelope, HostRequestEnvelope, TranscriptCapabilityObservation,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Record)]
pub struct PersistedEffectLeaseIdentity {
    pub intent_id: EffectIntentId,
    pub authorizing_activity_id: ActivityId,
    pub correlation_id: ActivityCorrelationId,
    pub attempt_id: EffectAttemptId,
    pub lease_id: EffectLeaseId,
    pub fence: u64,
    pub expires_at: UnixTimestampMilliseconds,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct LeasedHostRequestEnvelope {
    pub lease: PersistedEffectLeaseIdentity,
    pub request: HostRequestEnvelope,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct LeasedHostObservationEnvelope {
    pub lease: PersistedEffectLeaseIdentity,
    pub observation: HostObservationEnvelope,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DurableTranscriptHostObservation {
    pub request_id: pod0_domain::HostRequestId,
    pub cancellation_id: pod0_domain::CancellationId,
    pub observed_request_revision: pod0_domain::StateRevision,
    pub sequence_number: u64,
    pub observed_at: UnixTimestampMilliseconds,
    pub observation: TranscriptCapabilityObservation,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DurableRecallHostObservation {
    pub request_id: pod0_domain::HostRequestId,
    pub cancellation_id: pod0_domain::CancellationId,
    pub observed_request_revision: pod0_domain::StateRevision,
    pub sequence_number: u64,
    pub observed_at: UnixTimestampMilliseconds,
    pub episode_id: pod0_domain::EpisodeId,
    pub generation_id: pod0_domain::EvidenceGenerationId,
    pub embeddings: Vec<crate::RecallSpanEmbeddingObservation>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DurableAgentModelHostObservation {
    pub request_id: pod0_domain::HostRequestId,
    pub cancellation_id: pod0_domain::CancellationId,
    pub observed_request_revision: pod0_domain::StateRevision,
    pub sequence_number: u64,
    pub observed_at: UnixTimestampMilliseconds,
    pub outcome: DurableAgentModelOutcome,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum DurableAgentModelOutcome {
    Completed {
        turn_id: pod0_domain::AgentTurnId,
        model_fence_id: pod0_domain::AgentExecutionFenceId,
        assistant_text: String,
        proposed_tool_call: Option<crate::AgentModelToolCallObservation>,
        usage: Option<crate::AgentModelUsageObservation>,
    },
    Failed {
        code: crate::HostFailureCode,
        safe_detail: Option<String>,
    },
    Cancelled,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DurableAgentApprovalHostObservation {
    pub request_id: pod0_domain::HostRequestId,
    pub cancellation_id: pod0_domain::CancellationId,
    pub observed_request_revision: pod0_domain::StateRevision,
    pub sequence_number: u64,
    pub observed_at: UnixTimestampMilliseconds,
    pub turn_id: pod0_domain::AgentTurnId,
    pub proposal_id: pod0_domain::AgentProposalId,
    pub proposal_digest: pod0_domain::ContentDigest,
    pub decision: crate::AgentApprovalDecision,
}

impl DurableAgentApprovalHostObservation {
    pub fn from_host(value: &HostObservationEnvelope) -> Option<Self> {
        let HostObservation::AgentApprovalObserved {
            turn_id,
            proposal_id,
            proposal_digest,
            decision,
        } = &value.observation
        else {
            return None;
        };
        Some(Self {
            request_id: value.request_id,
            cancellation_id: value.cancellation_id,
            observed_request_revision: value.observed_request_revision,
            sequence_number: value.sequence_number,
            observed_at: value.observed_at,
            turn_id: *turn_id,
            proposal_id: *proposal_id,
            proposal_digest: *proposal_digest,
            decision: *decision,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DurableAgentCapabilityHostObservation {
    pub request_id: pod0_domain::HostRequestId,
    pub cancellation_id: pod0_domain::CancellationId,
    pub observed_request_revision: pod0_domain::StateRevision,
    pub sequence_number: u64,
    pub observed_at: UnixTimestampMilliseconds,
    pub outcome: DurableAgentCapabilityOutcome,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum DurableAgentCapabilityOutcome {
    Observed {
        turn_id: pod0_domain::AgentTurnId,
        proposal_id: pod0_domain::AgentProposalId,
        execution_fence_id: pod0_domain::AgentExecutionFenceId,
        outcome: crate::AgentCapabilityOutcome,
    },
    Failed {
        code: crate::HostFailureCode,
        safe_detail: Option<String>,
    },
    Cancelled,
}

impl DurableAgentCapabilityHostObservation {
    pub fn from_host(value: &HostObservationEnvelope) -> Option<Self> {
        let outcome = match &value.observation {
            HostObservation::AgentCapabilityObserved {
                turn_id,
                proposal_id,
                execution_fence_id,
                outcome,
            } => DurableAgentCapabilityOutcome::Observed {
                turn_id: *turn_id,
                proposal_id: *proposal_id,
                execution_fence_id: *execution_fence_id,
                outcome: outcome.clone(),
            },
            HostObservation::Failed { code, safe_detail } => {
                DurableAgentCapabilityOutcome::Failed {
                    code: *code,
                    safe_detail: safe_detail.clone(),
                }
            }
            HostObservation::Cancelled => DurableAgentCapabilityOutcome::Cancelled,
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

impl DurableAgentModelHostObservation {
    pub fn from_host(value: &HostObservationEnvelope) -> Option<Self> {
        let outcome = match &value.observation {
            HostObservation::AgentModelCompleted {
                turn_id,
                model_fence_id,
                assistant_text,
                proposed_tool_call,
                usage,
            } => DurableAgentModelOutcome::Completed {
                turn_id: *turn_id,
                model_fence_id: *model_fence_id,
                assistant_text: assistant_text.clone(),
                proposed_tool_call: proposed_tool_call.clone(),
                usage: *usage,
            },
            HostObservation::Failed { code, safe_detail } => DurableAgentModelOutcome::Failed {
                code: *code,
                safe_detail: safe_detail.clone(),
            },
            HostObservation::Cancelled => DurableAgentModelOutcome::Cancelled,
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

impl DurableRecallHostObservation {
    pub fn from_host(value: &HostObservationEnvelope) -> Option<Self> {
        let HostObservation::RecallSpansEmbedded {
            episode_id,
            generation_id,
            embeddings,
        } = &value.observation
        else {
            return None;
        };
        Some(Self {
            request_id: value.request_id,
            cancellation_id: value.cancellation_id,
            observed_request_revision: value.observed_request_revision,
            sequence_number: value.sequence_number,
            observed_at: value.observed_at,
            episode_id: *episode_id,
            generation_id: *generation_id,
            embeddings: embeddings.clone(),
        })
    }

    #[must_use]
    pub fn into_host_observation(self) -> HostObservation {
        HostObservation::RecallSpansEmbedded {
            episode_id: self.episode_id,
            generation_id: self.generation_id,
            embeddings: self.embeddings,
        }
    }
}

impl DurableTranscriptHostObservation {
    pub fn from_host(value: &HostObservationEnvelope) -> Option<Self> {
        let HostObservation::TranscriptCapabilityObserved { observation } = &value.observation
        else {
            return None;
        };
        Some(Self {
            request_id: value.request_id,
            cancellation_id: value.cancellation_id,
            observed_request_revision: value.observed_request_revision,
            sequence_number: value.sequence_number,
            observed_at: value.observed_at,
            observation: observation.clone(),
        })
    }

    #[must_use]
    pub fn into_host(self) -> HostObservationEnvelope {
        HostObservationEnvelope {
            request_id: self.request_id,
            cancellation_id: self.cancellation_id,
            observed_request_revision: self.observed_request_revision,
            sequence_number: self.sequence_number,
            observed_at: self.observed_at,
            observation: HostObservation::TranscriptCapabilityObserved {
                observation: self.observation,
            },
        }
    }
}
