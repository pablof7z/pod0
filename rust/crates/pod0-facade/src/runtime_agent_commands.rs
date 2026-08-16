use pod0_application::{
    AgentTurnStage, AgentTurnStart, AgentTurnState, ApplicationCommand, CommandEnvelope,
    CoreFailureCode, OperationResult, RequestDisposition, RequestRejectionReason,
    product_proof_agent_tools,
};
use pod0_domain::{AgentTurnId, ConversationId, StateRevision};
use pod0_storage::AgentCommandContext;

use crate::runtime_agent_modules::identity::{agent_fingerprint, agent_turn_id, model_fence_id};
use crate::runtime_state::FacadeState;
use crate::runtime_storage_commands::storage_failure;

pub(crate) const MAX_ACTIVE_AGENT_TURNS: u16 = 32;

impl FacadeState {
    pub(crate) fn accept_agent_command(
        &mut self,
        envelope: &CommandEnvelope,
        command: ApplicationCommand,
        fingerprint: &str,
    ) {
        match command {
            ApplicationCommand::StartAgentTurn {
                conversation_id,
                user_input,
                model_reference,
            } => self.start_agent_turn(
                envelope,
                fingerprint,
                conversation_id,
                user_input,
                model_reference,
            ),
            ApplicationCommand::CancelAgentTurn {
                turn_id,
                expected_turn_revision,
            } => self.cancel_agent_turn(envelope, fingerprint, turn_id, expected_turn_revision),
            _ => unreachable!("agent dispatcher received another command"),
        }
    }

    fn start_agent_turn(
        &mut self,
        envelope: &CommandEnvelope,
        fingerprint: &str,
        conversation_id: Option<ConversationId>,
        user_input: String,
        model_reference: String,
    ) {
        let Some(store) = self.agent_store.clone() else {
            self.fail(envelope.command_id, CoreFailureCode::StorageUnavailable);
            return;
        };
        match store.recoverable_turns(MAX_ACTIVE_AGENT_TURNS) {
            Ok(active) if active.len() >= usize::from(MAX_ACTIVE_AGENT_TURNS) => {
                self.fail(envelope.command_id, CoreFailureCode::HostUnavailable);
                return;
            }
            Err(error) => {
                self.fail(envelope.command_id, storage_failure(error));
                return;
            }
            Ok(_) => {}
        }
        let now = self.now();
        let turn_id = agent_turn_id(envelope.command_id);
        let conversation_id =
            conversation_id.unwrap_or_else(|| ConversationId::from_bytes(turn_id.into_bytes()));
        let state = AgentTurnState::start(AgentTurnStart {
            conversation_id,
            turn_id,
            model_fence_id: model_fence_id(turn_id),
            user_input,
            model_reference,
            available_tools: product_proof_agent_tools(),
            cancellation_id: envelope.cancellation_id,
            observed_at: now,
        });
        let Ok(state) = state else {
            self.fail(envelope.command_id, CoreFailureCode::InvalidCommand);
            return;
        };
        let context = AgentCommandContext {
            command_id: envelope.command_id,
            command_fingerprint: agent_fingerprint(
                b"pod0:agent-start-command:v1",
                &[fingerprint.as_bytes()],
            ),
            observed_at: now,
        };
        match store.start_turn_activity(context, &state) {
            Ok(outcome) => {
                let persisted = outcome.state().clone();
                debug_assert_eq!(persisted.projection().stage, AgentTurnStage::AwaitingModel);
                self.succeed(
                    envelope.command_id,
                    Some(OperationResult::AgentTurnStarted {
                        conversation_id,
                        turn_id,
                    }),
                );
            }
            Err(error) => self.fail(envelope.command_id, storage_failure(error)),
        }
    }

    fn cancel_agent_turn(
        &mut self,
        envelope: &CommandEnvelope,
        fingerprint: &str,
        turn_id: AgentTurnId,
        expected_revision: StateRevision,
    ) {
        let Some(store) = self.agent_store.clone() else {
            self.fail(envelope.command_id, CoreFailureCode::StorageUnavailable);
            return;
        };
        let result = store.cancel_turn_activity(
            AgentCommandContext {
                command_id: envelope.command_id,
                command_fingerprint: agent_fingerprint(
                    b"pod0:agent-cancel-command:v2",
                    &[fingerprint.as_bytes()],
                ),
                observed_at: self.now(),
            },
            turn_id,
            expected_revision,
        );
        match result {
            Ok(outcome) if outcome.disposition == RequestDisposition::Accepted => {
                if let Some(cancellation_id) = outcome.cancellation_id {
                    self.cancel_operation(cancellation_id);
                }
                self.succeed(envelope.command_id, None);
            }
            Ok(outcome) => self.fail(
                envelope.command_id,
                match outcome.disposition {
                    RequestDisposition::Rejected {
                        reason: RequestRejectionReason::MissingSubject,
                    } => CoreFailureCode::NotFound,
                    RequestDisposition::Rejected {
                        reason: RequestRejectionReason::RevisionConflict,
                    }
                    | RequestDisposition::Stale => CoreFailureCode::RevisionConflict,
                    RequestDisposition::AlreadyComplete | RequestDisposition::Duplicate => {
                        CoreFailureCode::Cancelled
                    }
                    _ => CoreFailureCode::InvalidCommand,
                },
            ),
            Err(error) => self.fail(envelope.command_id, storage_failure(error)),
        }
    }
}
