use pod0_application::{
    HostObservation, HostObservationEnvelope, HostObservationReceipt, HostObservationRejection,
    ScheduledAgentExecutionObservation,
};
use pod0_storage::{ScheduledAgentObservationInput, ScheduledAgentObservationOutcome};

use crate::runtime_state::FacadeState;

impl FacadeState {
    pub(crate) fn retry_pending_scheduled_agent_observation(
        &mut self,
        request_id: pod0_domain::HostRequestId,
        observation: &HostObservationEnvelope,
    ) -> Option<(bool, HostObservationReceipt)> {
        let pending = self
            .pending_scheduled_agent_observations
            .get(&request_id)?
            .clone();
        if &pending != observation {
            return Some((false, retain(request_id)));
        }
        let receipt = self.persist_scheduled_agent_observation(observation.clone());
        let changed = matches!(receipt, HostObservationReceipt::Persisted { .. });
        if !matches!(receipt, HostObservationReceipt::RetainAndRetry { .. }) {
            self.pending_scheduled_agent_observations
                .remove(&request_id);
        }
        Some((changed, receipt))
    }

    pub(crate) fn persist_scheduled_agent_observation(
        &mut self,
        envelope: HostObservationEnvelope,
    ) -> HostObservationReceipt {
        let request_id = envelope.request_id;
        if !self.pending_scheduled_agents.contains_key(&request_id) {
            return rejected(request_id, HostObservationRejection::UnknownRequest);
        }
        let Some(store) = self.scheduled_agent_store.clone() else {
            return retain(request_id);
        };
        let HostObservation::ScheduledAgentExecutionObserved { observation } = envelope.observation
        else {
            return rejected(request_id, HostObservationRejection::MismatchedPayload);
        };
        let terminal = !matches!(
            observation,
            ScheduledAgentExecutionObservation::Accepted { .. }
        );
        let outcome = store.apply_observation(ScheduledAgentObservationInput {
            request_id,
            cancellation_id: envelope.cancellation_id,
            observed_request_revision: envelope.observed_request_revision,
            sequence_number: envelope.sequence_number,
            observed_at: envelope.observed_at,
            observation,
        });
        match outcome {
            Ok(ScheduledAgentObservationOutcome::Updated(state)) => {
                self.revision =
                    pod0_domain::StateRevision::new(self.revision.value.max(state.revision.value));
                if terminal {
                    self.retire_scheduled_agent_request(request_id);
                }
                HostObservationReceipt::Persisted {
                    request_id,
                    terminal,
                }
            }
            Ok(ScheduledAgentObservationOutcome::Duplicate(_)) => {
                HostObservationReceipt::Persisted {
                    request_id,
                    terminal,
                }
            }
            Ok(ScheduledAgentObservationOutcome::Stale) => {
                rejected(request_id, HostObservationRejection::StaleWorkflow)
            }
            Err(_) => retain(request_id),
        }
    }
}

fn retain(request_id: pod0_domain::HostRequestId) -> HostObservationReceipt {
    HostObservationReceipt::RetainAndRetry { request_id }
}

fn rejected(
    request_id: pod0_domain::HostRequestId,
    reason: HostObservationRejection,
) -> HostObservationReceipt {
    HostObservationReceipt::Rejected { request_id, reason }
}
