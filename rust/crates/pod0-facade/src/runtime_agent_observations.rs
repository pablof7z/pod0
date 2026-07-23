use pod0_application::{
    AgentActionObservation, AgentActionOutcome, AgentAuthorizationObservation,
    AgentModelObservation, AgentTurnStage, AgentWorkflowAcceptance, HostObservation,
    HostObservationEnvelope, HostObservationReceipt, HostObservationRejection,
    parse_agent_tool_call,
};
use pod0_domain::{CommandId, HostRequestId};
use pod0_storage::{AgentAuditKind, AgentStore, StorageError};

use crate::runtime_agent_modules::identity::agent_authorization_id;
use crate::runtime_agent_modules::identity::continuation_model_fence_id;
use crate::runtime_agent_modules::observation_values::{
    is_terminal, map_capability_outcome, rejected, retain,
};
use crate::runtime_agent_modules::persistence::persist_agent_update;
use crate::runtime_state::FacadeState;

impl FacadeState {
    pub(crate) fn retry_pending_agent_observation(
        &mut self,
        request_id: HostRequestId,
        observation: &HostObservationEnvelope,
    ) -> Option<(bool, HostObservationReceipt)> {
        let pending = self.pending_agent_observations.get(&request_id)?;
        if pending != observation {
            return Some((false, retain(request_id)));
        }
        let receipt = self.persist_agent_observation(observation.clone());
        let changed = matches!(receipt, HostObservationReceipt::Persisted { .. });
        if !matches!(receipt, HostObservationReceipt::RetainAndRetry { .. }) {
            self.pending_agent_observations.remove(&request_id);
        }
        Some((changed, receipt))
    }

    pub(crate) fn accept_agent_observation(
        &mut self,
        observation: HostObservationEnvelope,
    ) -> (bool, HostObservationReceipt) {
        let request_id = observation.request_id;
        let retained = observation.clone();
        let receipt = self.persist_agent_observation(observation);
        if matches!(receipt, HostObservationReceipt::RetainAndRetry { .. }) {
            self.pending_agent_observations.insert(request_id, retained);
        }
        let changed = matches!(receipt, HostObservationReceipt::Persisted { .. });
        (changed, receipt)
    }

    fn persist_agent_observation(
        &mut self,
        envelope: HostObservationEnvelope,
    ) -> HostObservationReceipt {
        let request_id = envelope.request_id;
        let Some(pending) = self.pending_agents.get(&request_id).cloned() else {
            return rejected(request_id, HostObservationRejection::UnknownRequest);
        };
        let Some(store) = self.agent_store.clone() else {
            return retain(request_id);
        };
        let result = store
            .turn(pending.turn_id)
            .and_then(|state| state.ok_or(StorageError::AgentTurnNotFound))
            .and_then(|state| self.fold_agent_observation(&store, state, &envelope));
        match result {
            Ok(state) => {
                let stage = state.projection().stage;
                let next = match stage {
                    AgentTurnStage::AwaitingModel => {
                        let command_id = CommandId::from_bytes(request_id.into_bytes());
                        let _ = self.queue_agent_model_request(command_id, &state);
                        Ok(())
                    }
                    AgentTurnStage::ApprovalRequired => {
                        self.queue_agent_approval_request(
                            CommandId::from_bytes(request_id.into_bytes()),
                            &state,
                        );
                        Ok(())
                    }
                    AgentTurnStage::Authorized => self.begin_authorized_agent_action(
                        &store,
                        state.clone(),
                        envelope.observed_at,
                    ),
                    _ => Ok(()),
                };
                if next.is_err() {
                    return retain(request_id);
                }
                self.retire_agent_request(request_id);
                self.advance_revision();
                HostObservationReceipt::Persisted {
                    request_id,
                    terminal: is_terminal(stage),
                }
            }
            Err(StorageError::AgentTurnConflict) => {
                rejected(request_id, HostObservationRejection::StaleWorkflow)
            }
            Err(_) => retain(request_id),
        }
    }

    fn fold_agent_observation(
        &mut self,
        store: &AgentStore,
        mut state: pod0_application::AgentTurnState,
        envelope: &HostObservationEnvelope,
    ) -> Result<pod0_application::AgentTurnState, StorageError> {
        let before = state.projection();
        let (acceptance, audit_kind, fingerprint_domain) = match &envelope.observation {
            HostObservation::AgentModelCompleted {
                turn_id,
                model_fence_id,
                assistant_text,
                proposed_tool_call,
                usage,
            } => {
                let proposed_action = match proposed_tool_call {
                    Some(call) => match parse_agent_tool_call(call) {
                        Ok(action) => Some(action),
                        Err(_) => {
                            return fail_invalid_model_action(
                                store,
                                state,
                                envelope,
                                before.revision,
                            );
                        }
                    },
                    None => None,
                };
                (
                    state.observe_model(AgentModelObservation {
                        turn_id: *turn_id,
                        model_fence_id: *model_fence_id,
                        assistant_text: assistant_text.clone(),
                        proposed_action,
                        usage: *usage,
                        observed_at: envelope.observed_at,
                    }),
                    AgentAuditKind::ModelObserved,
                    b"pod0:agent-model-observation:v2".as_slice(),
                )
            }
            HostObservation::AgentApprovalObserved {
                proposal_id,
                proposal_digest,
                approved,
                ..
            } => {
                let authority = before
                    .proposal
                    .as_ref()
                    .map(|proposal| proposal.required_authority)
                    .ok_or(StorageError::InvalidAgentState)?;
                (
                    state.authorize(AgentAuthorizationObservation {
                        proposal_id: *proposal_id,
                        proposal_digest: *proposal_digest,
                        authority,
                        authorization_id: agent_authorization_id(envelope.request_id),
                        approved: *approved,
                        observed_at: envelope.observed_at,
                    }),
                    AgentAuditKind::AuthorizationObserved,
                    b"pod0:agent-authorization-observation:v1".as_slice(),
                )
            }
            HostObservation::AgentCapabilityObserved {
                proposal_id,
                execution_fence_id,
                outcome,
                ..
            } => (
                state.observe_action(AgentActionObservation {
                    proposal_id: *proposal_id,
                    execution_fence_id: *execution_fence_id,
                    outcome: map_capability_outcome(outcome.clone()),
                    observed_at: envelope.observed_at,
                }),
                AgentAuditKind::ActionObserved,
                b"pod0:agent-capability-observation:v1".as_slice(),
            ),
            HostObservation::Failed { safe_detail, .. }
                if before.stage == AgentTurnStage::AwaitingModel =>
            {
                (
                    state.fail_model(safe_detail.clone(), envelope.observed_at),
                    AgentAuditKind::ModelObserved,
                    b"pod0:agent-model-failure:v1".as_slice(),
                )
            }
            HostObservation::Failed { safe_detail, .. }
                if before.stage == AgentTurnStage::Executing =>
            {
                let proposal = before
                    .proposal
                    .as_ref()
                    .ok_or(StorageError::InvalidAgentState)?;
                let fence = before
                    .execution_fence_id
                    .ok_or(StorageError::InvalidAgentState)?;
                (
                    state.observe_action(AgentActionObservation {
                        proposal_id: proposal.proposal_id,
                        execution_fence_id: fence,
                        outcome: AgentActionOutcome::Failed {
                            safe_detail: safe_detail.clone(),
                        },
                        observed_at: envelope.observed_at,
                    }),
                    AgentAuditKind::ActionObserved,
                    b"pod0:agent-capability-failure:v1".as_slice(),
                )
            }
            HostObservation::Cancelled => (
                state.cancel(envelope.observed_at),
                AgentAuditKind::Cancelled,
                b"pod0:agent-host-cancelled:v1".as_slice(),
            ),
            _ => return Err(StorageError::AgentTurnConflict),
        };
        match acceptance {
            AgentWorkflowAcceptance::Updated | AgentWorkflowAcceptance::Duplicate => {}
            AgentWorkflowAcceptance::Rejected
                if state.projection().revision.value > before.revision.value => {}
            AgentWorkflowAcceptance::Stale | AgentWorkflowAcceptance::Rejected => {
                return Err(StorageError::AgentTurnConflict);
            }
        }
        if acceptance == AgentWorkflowAcceptance::Updated
            && state.projection().stage == AgentTurnStage::Committed
        {
            let projection = state.projection();
            let continuation_fence =
                continuation_model_fence_id(projection.turn_id, projection.revision);
            if state.continue_after_commit(continuation_fence, envelope.observed_at)
                != AgentWorkflowAcceptance::Updated
            {
                return Err(StorageError::AgentTurnConflict);
            }
        }
        persist_agent_update(
            store,
            CommandId::from_bytes(envelope.request_id.into_bytes()),
            fingerprint_domain,
            audit_kind,
            before.revision,
            state,
            envelope.observed_at,
        )
    }
}

fn fail_invalid_model_action(
    store: &AgentStore,
    mut state: pod0_application::AgentTurnState,
    envelope: &HostObservationEnvelope,
    expected_revision: pod0_domain::StateRevision,
) -> Result<pod0_application::AgentTurnState, StorageError> {
    if state.fail_model(Some("invalid_tool_action".into()), envelope.observed_at)
        != AgentWorkflowAcceptance::Updated
    {
        return Err(StorageError::AgentTurnConflict);
    }
    persist_agent_update(
        store,
        CommandId::from_bytes(envelope.request_id.into_bytes()),
        b"pod0:agent-model-invalid-action:v1",
        AgentAuditKind::ModelObserved,
        expected_revision,
        state,
        envelope.observed_at,
    )
}
