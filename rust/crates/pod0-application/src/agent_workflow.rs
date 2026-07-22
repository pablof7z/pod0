use pod0_domain::{
    AgentCommitId, AgentExecutionFenceId, AgentProposalId, ContentDigest, StateRevision,
    UnixTimestampMilliseconds,
};
use sha2::{Digest as _, Sha256};

use crate::{
    AgentActionObservation, AgentActionOutcome, AgentAuthority, AgentAuthorizationObservation,
    AgentCommitReceipt, AgentMessageProjection, AgentMessageRole, AgentModelObservation,
    AgentProposalProjection, AgentTurnProjection, AgentTurnStage, AgentTurnStart,
    AgentTurnStartError, AgentTurnState, AgentWorkflowAcceptance, MAX_AGENT_INPUT_BYTES,
    MAX_AGENT_MESSAGE_BYTES, agent_proposal_identity, agent_tool_policy, validate_agent_action,
    validate_agent_model_reference,
};

impl AgentTurnState {
    pub fn start(input: AgentTurnStart) -> Result<Self, AgentTurnStartError> {
        if input.user_input.trim().is_empty() || input.user_input.len() > MAX_AGENT_INPUT_BYTES {
            return Err(AgentTurnStartError::InvalidInput);
        }
        if validate_agent_model_reference(&input.model_reference).is_err() {
            return Err(AgentTurnStartError::InvalidModelReference);
        }
        Ok(Self {
            projection: AgentTurnProjection {
                conversation_id: input.conversation_id,
                turn_id: input.turn_id,
                revision: StateRevision::new(1),
                stage: AgentTurnStage::AwaitingModel,
                messages: vec![AgentMessageProjection {
                    role: AgentMessageRole::User,
                    content: input.user_input,
                }],
                proposal: None,
                execution_fence_id: Some(input.model_fence_id),
                commit: None,
                safe_failure: None,
                updated_at: input.observed_at,
            },
            model_fence_id: input.model_fence_id,
            authorization_id: None,
            action_observation: None,
            model_reference: input.model_reference,
        })
    }

    #[must_use]
    pub fn projection(&self) -> AgentTurnProjection {
        self.projection.clone()
    }

    #[must_use]
    pub fn model_reference(&self) -> &str {
        &self.model_reference
    }

    #[must_use]
    pub fn is_valid_for_recovery(&self) -> bool {
        validate_agent_model_reference(&self.model_reference).is_ok()
            && !self.projection.messages.is_empty()
            && self.projection.messages.iter().all(|message| {
                !message.content.is_empty() && message.content.len() <= MAX_AGENT_MESSAGE_BYTES
            })
            && self.projection.proposal.as_ref().is_none_or(|proposal| {
                validate_agent_action(&proposal.action).is_ok()
                    && agent_proposal_identity(
                        self.projection.turn_id,
                        proposal.revision,
                        &proposal.action,
                    ) == (proposal.proposal_id, proposal.proposal_digest)
                    && agent_tool_policy(proposal.action.tool()).authority
                        == proposal.required_authority
            })
    }

    pub fn observe_model(&mut self, observation: AgentModelObservation) -> AgentWorkflowAcceptance {
        if observation.turn_id != self.projection.turn_id
            || observation.model_fence_id != self.model_fence_id
        {
            return AgentWorkflowAcceptance::Stale;
        }
        if self.projection.stage != AgentTurnStage::AwaitingModel {
            return AgentWorkflowAcceptance::Duplicate;
        }
        if observation.assistant_text.len() > MAX_AGENT_MESSAGE_BYTES
            || observation.assistant_text.trim().is_empty() && observation.proposed_action.is_none()
        {
            self.fail("invalid_model_output", observation.observed_at);
            return AgentWorkflowAcceptance::Rejected;
        }
        if !observation.assistant_text.trim().is_empty() {
            self.projection.messages.push(AgentMessageProjection {
                role: AgentMessageRole::Assistant,
                content: observation.assistant_text,
            });
        }
        let Some(action) = observation.proposed_action else {
            self.advance(AgentTurnStage::Completed, observation.observed_at);
            self.projection.execution_fence_id = None;
            return AgentWorkflowAcceptance::Updated;
        };
        if validate_agent_action(&action).is_err() {
            self.fail("invalid_tool_action", observation.observed_at);
            return AgentWorkflowAcceptance::Rejected;
        }
        let next_revision = StateRevision::new(self.projection.revision.value + 1);
        let (proposal_id, proposal_digest) =
            agent_proposal_identity(self.projection.turn_id, next_revision, &action);
        let required_authority = agent_tool_policy(action.tool()).authority;
        self.projection.proposal = Some(AgentProposalProjection {
            proposal_id,
            proposal_digest,
            revision: next_revision,
            action,
            required_authority,
        });
        self.projection.execution_fence_id = None;
        self.advance(
            if required_authority == AgentAuthority::None {
                AgentTurnStage::Authorized
            } else {
                AgentTurnStage::ApprovalRequired
            },
            observation.observed_at,
        );
        AgentWorkflowAcceptance::Updated
    }

    pub fn authorize(
        &mut self,
        observation: AgentAuthorizationObservation,
    ) -> AgentWorkflowAcceptance {
        let Some(proposal) = self.projection.proposal.as_ref() else {
            return AgentWorkflowAcceptance::Stale;
        };
        if proposal.proposal_id != observation.proposal_id
            || proposal.proposal_digest != observation.proposal_digest
        {
            return AgentWorkflowAcceptance::Stale;
        }
        if self.projection.stage != AgentTurnStage::ApprovalRequired {
            return if self.authorization_id == Some(observation.authorization_id) {
                AgentWorkflowAcceptance::Duplicate
            } else {
                AgentWorkflowAcceptance::Rejected
            };
        }
        if observation.authority != proposal.required_authority {
            return AgentWorkflowAcceptance::Rejected;
        }
        self.authorization_id = Some(observation.authorization_id);
        self.advance(
            if observation.approved {
                AgentTurnStage::Authorized
            } else {
                AgentTurnStage::Denied
            },
            observation.observed_at,
        );
        AgentWorkflowAcceptance::Updated
    }

    pub fn begin_execution(
        &mut self,
        fence_id: AgentExecutionFenceId,
        observed_at: UnixTimestampMilliseconds,
    ) -> AgentWorkflowAcceptance {
        if self.projection.stage == AgentTurnStage::Executing
            && self.projection.execution_fence_id == Some(fence_id)
        {
            return AgentWorkflowAcceptance::Duplicate;
        }
        if self.projection.stage != AgentTurnStage::Authorized {
            return AgentWorkflowAcceptance::Rejected;
        }
        self.projection.execution_fence_id = Some(fence_id);
        self.advance(AgentTurnStage::Executing, observed_at);
        AgentWorkflowAcceptance::Updated
    }

    pub fn observe_action(
        &mut self,
        observation: AgentActionObservation,
    ) -> AgentWorkflowAcceptance {
        let Some(proposal) = self.projection.proposal.as_ref() else {
            return AgentWorkflowAcceptance::Stale;
        };
        if proposal.proposal_id != observation.proposal_id
            || self.projection.execution_fence_id != Some(observation.execution_fence_id)
        {
            return AgentWorkflowAcceptance::Stale;
        }
        if let Some(existing) = &self.action_observation {
            return if existing == &observation {
                AgentWorkflowAcceptance::Duplicate
            } else {
                AgentWorkflowAcceptance::Rejected
            };
        }
        if self.projection.stage != AgentTurnStage::Executing {
            return AgentWorkflowAcceptance::Rejected;
        }
        self.action_observation = Some(observation.clone());
        match observation.outcome {
            AgentActionOutcome::Succeeded {
                bounded_result,
                artifact_id,
            } => {
                if bounded_result.len() > MAX_AGENT_MESSAGE_BYTES {
                    self.fail("tool_result_too_large", observation.observed_at);
                    return AgentWorkflowAcceptance::Rejected;
                }
                self.projection.messages.push(AgentMessageProjection {
                    role: AgentMessageRole::Tool,
                    content: bounded_result,
                });
                let commit_id = commit_id(proposal.proposal_id, proposal.proposal_digest);
                self.projection.commit = Some(AgentCommitReceipt {
                    commit_id,
                    proposal_id: proposal.proposal_id,
                    artifact_id,
                    committed_at: observation.observed_at,
                });
                self.advance(AgentTurnStage::Committed, observation.observed_at);
            }
            AgentActionOutcome::Failed { safe_detail } => {
                self.projection.safe_failure = safe_detail.map(|value| {
                    value
                        .chars()
                        .take(crate::MAX_AGENT_SAFE_DETAIL_BYTES)
                        .collect()
                });
                self.advance(AgentTurnStage::Failed, observation.observed_at);
            }
            AgentActionOutcome::Cancelled => {
                self.advance(AgentTurnStage::Cancelled, observation.observed_at)
            }
            AgentActionOutcome::OutcomeAmbiguous => {
                self.advance(AgentTurnStage::OutcomeAmbiguous, observation.observed_at)
            }
        }
        AgentWorkflowAcceptance::Updated
    }

    pub fn cancel(&mut self, observed_at: UnixTimestampMilliseconds) -> AgentWorkflowAcceptance {
        if matches!(
            self.projection.stage,
            AgentTurnStage::Committed | AgentTurnStage::Completed
        ) {
            return AgentWorkflowAcceptance::Rejected;
        }
        if self.projection.stage == AgentTurnStage::Cancelled {
            return AgentWorkflowAcceptance::Duplicate;
        }
        self.advance(AgentTurnStage::Cancelled, observed_at);
        AgentWorkflowAcceptance::Updated
    }

    fn advance(&mut self, stage: AgentTurnStage, observed_at: UnixTimestampMilliseconds) {
        self.projection.revision = StateRevision::new(self.projection.revision.value + 1);
        self.projection.stage = stage;
        self.projection.updated_at = observed_at;
    }

    fn fail(&mut self, code: &str, observed_at: UnixTimestampMilliseconds) {
        self.projection.safe_failure = Some(code.to_owned());
        self.projection.execution_fence_id = None;
        self.advance(AgentTurnStage::Failed, observed_at);
    }
}

fn commit_id(proposal_id: AgentProposalId, digest: ContentDigest) -> AgentCommitId {
    let mut hasher = Sha256::new();
    hasher.update(b"pod0:agent-commit:v1\0");
    hasher.update(proposal_id.into_bytes());
    hasher.update(digest.into_bytes());
    let bytes: [u8; 32] = hasher.finalize().into();
    let mut id = [0_u8; 16];
    id.copy_from_slice(&bytes[..16]);
    AgentCommitId::from_bytes(id)
}
