use pod0_application::{
    HostObservation, HostObservationReceipt, HostObservationRejection,
    LeasedHostObservationEnvelope, ScheduledAgentExecutionObservation,
};
use pod0_storage::{
    ScheduledAgentLeasedObservationInput, ScheduledAgentObservationInput,
    ScheduledAgentObservationOutcome,
};

use crate::runtime_chapter_model_receipts::{persisted, retain};
use crate::runtime_leased_observations::rejected_payload;
use crate::runtime_state::FacadeState;

impl FacadeState {
    pub(crate) fn record_leased_scheduled_agent_observation(
        &mut self,
        leased: LeasedHostObservationEnvelope,
    ) -> (bool, HostObservationReceipt) {
        let request_id = leased.observation.request_id;
        let HostObservation::ScheduledAgentExecutionObserved { observation } =
            leased.observation.observation.clone()
        else {
            return (
                false,
                rejected_payload(request_id, HostObservationRejection::MismatchedPayload),
            );
        };
        let terminal = !matches!(
            observation,
            ScheduledAgentExecutionObservation::Accepted { .. }
        );
        let Some(store) = self.scheduled_agent_store.clone() else {
            return (false, retain(request_id));
        };
        let outcome = store.apply_leased_observation(ScheduledAgentLeasedObservationInput {
            lease: leased.lease,
            observation: ScheduledAgentObservationInput {
                request_id,
                cancellation_id: leased.observation.cancellation_id,
                observed_request_revision: leased.observation.observed_request_revision,
                sequence_number: leased.observation.sequence_number,
                observed_at: leased.observation.observed_at,
                observation,
            },
            committed_at: self.now(),
        });
        match outcome {
            Ok(ScheduledAgentObservationOutcome::Updated(state)) => {
                self.revision =
                    pod0_domain::StateRevision::new(self.revision.value.max(state.revision.value));
                if terminal {
                    self.host_requests.retire(request_id);
                }
                (true, persisted(request_id, terminal))
            }
            Ok(ScheduledAgentObservationOutcome::Duplicate(_)) => (
                false,
                rejected_payload(request_id, HostObservationRejection::Duplicate),
            ),
            Ok(ScheduledAgentObservationOutcome::Stale) => (
                false,
                rejected_payload(request_id, HostObservationRejection::StaleWorkflow),
            ),
            Err(pod0_storage::StorageError::StaleScheduledAgentAttempt) => (
                false,
                rejected_payload(request_id, HostObservationRejection::StaleWorkflow),
            ),
            Err(_) => (false, retain(request_id)),
        }
    }
}
