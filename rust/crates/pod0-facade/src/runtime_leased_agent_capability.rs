use pod0_application::{
    AgentCapabilityOutcome, DurableAgentCapabilityHostObservation, DurableAgentCapabilityOutcome,
    HostObservationReceipt, HostObservationRejection, LeasedHostObservationEnvelope,
};
use pod0_storage::AgentCapabilityObservationCommitInput;

use crate::runtime_chapter_model_receipts::{persisted, retain};
use crate::runtime_leased_observations::rejected_payload;
use crate::runtime_state::FacadeState;

impl FacadeState {
    pub(super) fn record_leased_agent_capability_observation(
        &mut self,
        leased: LeasedHostObservationEnvelope,
        durable: DurableAgentCapabilityHostObservation,
    ) -> (bool, HostObservationReceipt) {
        let request_id = durable.request_id;
        let Some(store) = self.agent_store.clone() else {
            return (false, retain(request_id));
        };
        let generated_audio = matches!(
            &durable.outcome,
            DurableAgentCapabilityOutcome::Observed {
                outcome: AgentCapabilityOutcome::GeneratedAudioStaged { .. },
                ..
            }
        );
        let committed =
            match store.commit_capability_observation(AgentCapabilityObservationCommitInput {
                lease: leased.lease,
                observation: durable,
                committed_at: self.now(),
            }) {
                Ok(value) => value,
                Err(pod0_storage::StorageError::AgentTurnConflict) => {
                    return (
                        false,
                        rejected_payload(request_id, HostObservationRejection::StaleWorkflow),
                    );
                }
                Err(_) => return (false, retain(request_id)),
            };
        if generated_audio && self.reload_listening().is_err() {
            return (false, retain(request_id));
        }
        if committed.replayed {
            return (
                false,
                rejected_payload(request_id, HostObservationRejection::Duplicate),
            );
        }
        self.host_requests.retire(request_id);
        self.advance_revision();
        (true, persisted(request_id, true))
    }
}
