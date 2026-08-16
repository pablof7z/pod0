use pod0_application::{
    DurableHostCancellationObservation, HostObservationReceipt, HostObservationRejection,
    LeasedHostObservationEnvelope,
};

use crate::runtime_leased_observations::rejected_payload;
use crate::runtime_state::FacadeState;

impl FacadeState {
    pub(super) fn record_leased_cancellation_observation(
        &mut self,
        leased: LeasedHostObservationEnvelope,
    ) -> (bool, HostObservationReceipt) {
        let request_id = leased.observation.request_id;
        let Some(observation) =
            DurableHostCancellationObservation::from_host(&leased.observation)
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
        match store.commit_cancellation_observation(
            pod0_storage::CancellationObservationCommitInput {
                lease: leased.lease,
                observation,
                committed_at: self.now(),
            },
        ) {
            Ok(outcome) => {
                self.host_requests.retire(request_id);
                (
                    !outcome.replayed,
                    HostObservationReceipt::Persisted {
                        request_id,
                        terminal: true,
                    },
                )
            }
            Err(pod0_storage::StorageError::CommandConflict) => (
                false,
                rejected_payload(request_id, HostObservationRejection::StaleWorkflow),
            ),
            Err(_) => (
                false,
                crate::runtime_chapter_model_receipts::retain(request_id),
            ),
        }
    }
}
