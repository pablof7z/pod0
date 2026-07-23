use pod0_application::{HostObservationEnvelope, HostObservationReceipt};
use pod0_domain::HostRequestId;

use crate::runtime_agent_modules::state::PendingAgentRecallObservation;
use crate::runtime_observation_mapping::retain;
use crate::runtime_recall_state::PendingRecall;
use crate::runtime_state::FacadeState;

impl FacadeState {
    pub(crate) fn retry_pending_agent_recall_observation(
        &mut self,
        observation: &HostObservationEnvelope,
    ) -> Option<(bool, HostObservationReceipt)> {
        let request_id = observation.request_id;
        let pending = self
            .pending_agent_recall_observations
            .get(&request_id)
            .cloned()?;
        if pending.envelope != *observation {
            return Some((false, retain(request_id)));
        }
        Some(
            match self.finish_agent_recall_if_terminal(pending.query_id, observation.observed_at) {
                Ok(true) => {
                    self.pending_agent_recall_observations.remove(&request_id);
                    (
                        true,
                        HostObservationReceipt::Persisted {
                            request_id,
                            terminal: true,
                        },
                    )
                }
                Ok(false) | Err(_) => (false, retain(request_id)),
            },
        )
    }

    pub(crate) fn finish_recall_observation_for_agent(
        &mut self,
        request_id: HostRequestId,
        pending: PendingRecall,
        observation: HostObservationEnvelope,
    ) -> bool {
        self.pending_recalls.remove(&request_id);
        let retained = observation.clone();
        self.finish_recall_observation(pending, observation.observation);
        if !self.pending_agent_recalls.contains_key(&pending.query_id)
            || self
                .finish_agent_recall_if_terminal(pending.query_id, observation.observed_at)
                .is_ok()
        {
            return false;
        }
        self.pending_agent_recall_observations.insert(
            request_id,
            PendingAgentRecallObservation {
                query_id: pending.query_id,
                envelope: retained,
            },
        );
        true
    }
}
