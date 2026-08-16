use pod0_application::{
    DurableLifecycleHostObservation, HostObservationReceipt, HostObservationRejection,
    LeasedHostObservationEnvelope,
};

use crate::runtime_leased_observations::rejected_payload;
use crate::runtime_state::FacadeState;

impl FacadeState {
    pub(super) fn record_leased_lifecycle_wake_observation(
        &mut self,
        leased: LeasedHostObservationEnvelope,
    ) -> (bool, HostObservationReceipt) {
        let request_id = leased.observation.request_id;
        let Some(observation) = DurableLifecycleHostObservation::from_host(&leased.observation)
        else {
            return (
                false,
                rejected_payload(request_id, HostObservationRejection::MismatchedPayload),
            );
        };
        let Some(store) = self.store.clone() else {
            return (
                false,
                crate::runtime_chapter_model_receipts::retain(request_id),
            );
        };
        let committed = store.commit_lifecycle_wake_observation(
            pod0_storage::LifecycleWakeObservationCommitInput {
                lease: leased.lease,
                observation,
                committed_at: self.now(),
            },
        );
        let outcome = match committed {
            Ok(outcome) => outcome,
            Err(pod0_storage::StorageError::CommandConflict) => {
                return (
                    false,
                    rejected_payload(request_id, HostObservationRejection::StaleWorkflow),
                );
            }
            Err(_) => {
                return (
                    false,
                    crate::runtime_chapter_model_receipts::retain(request_id),
                );
            }
        };
        self.host_requests.retire(request_id);
        let changed = outcome.reached && self.apply_core_wake_reaction(outcome.reason, true);
        (
            changed || !outcome.replayed,
            HostObservationReceipt::Persisted {
                request_id,
                terminal: true,
            },
        )
    }
}
