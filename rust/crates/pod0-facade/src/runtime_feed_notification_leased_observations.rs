use pod0_application::{
    DurableFeedHostObservation, DurableFeedObservationOutcome, HostFailureCode,
    HostObservationReceipt, HostObservationRejection, LeasedHostObservationEnvelope,
};
use pod0_storage::{FeedDiscoveryNotificationOutcome, FeedNotificationObservationCommitInput};

use crate::runtime_chapter_model_receipts::{persisted, retain};
use crate::runtime_leased_observations::rejected_payload;
use crate::runtime_state::FacadeState;

impl FacadeState {
    pub(super) fn record_leased_feed_notification_observation(
        &mut self,
        leased: LeasedHostObservationEnvelope,
    ) -> (bool, HostObservationReceipt) {
        let request_id = leased.observation.request_id;
        let Some(observation) = DurableFeedHostObservation::from_host(&leased.observation) else {
            return (
                false,
                rejected_payload(request_id, HostObservationRejection::MismatchedPayload),
            );
        };
        let Some(store) = self.store.clone() else {
            return (false, retain(request_id));
        };
        let Ok(Some(record)) = store
            .requested_feed_discovery_notifications(64)
            .map(|records| {
                records
                    .into_iter()
                    .find(|record| record.request_id == Some(request_id))
            })
        else {
            return (
                false,
                rejected_payload(request_id, HostObservationRejection::StaleWorkflow),
            );
        };
        let outcome = match observation.outcome {
            DurableFeedObservationOutcome::NotificationDelivered {
                occurrence_id,
                episode_id,
            } if occurrence_id == record.occurrence_id && episode_id == record.episode_id => {
                FeedDiscoveryNotificationOutcome::Delivered
            }
            DurableFeedObservationOutcome::Failed { code } => notification_failure(code),
            DurableFeedObservationOutcome::Cancelled => FeedDiscoveryNotificationOutcome::Cancelled,
            _ => {
                return (
                    false,
                    rejected_payload(request_id, HostObservationRejection::MismatchedPayload),
                );
            }
        };
        let result =
            store.commit_feed_notification_observation(FeedNotificationObservationCommitInput {
                lease: leased.lease,
                observation,
                outcome,
                committed_at: self.now(),
            });
        let Ok(committed) = result else {
            return (false, retain(request_id));
        };
        if committed.replayed {
            return (
                false,
                rejected_payload(request_id, HostObservationRejection::Duplicate),
            );
        }
        let _ = self.reload_listening();
        let _ = self.reconcile_feed_discovery_workflows();
        self.advance_revision();
        (true, persisted(request_id, true))
    }
}

fn notification_failure(code: HostFailureCode) -> FeedDiscoveryNotificationOutcome {
    match code {
        HostFailureCode::Offline
        | HostFailureCode::TimedOut
        | HostFailureCode::ProviderUnavailable
        | HostFailureCode::PlatformFailure => FeedDiscoveryNotificationOutcome::RetryableFailure,
        HostFailureCode::PermissionDenied | HostFailureCode::Unauthorized => {
            FeedDiscoveryNotificationOutcome::PermissionDenied
        }
        HostFailureCode::InvalidResponse
        | HostFailureCode::ResponseTooLarge
        | HostFailureCode::MediaUnavailable
        | HostFailureCode::IndexUnavailable
        | HostFailureCode::Unsupported { .. } => FeedDiscoveryNotificationOutcome::PermanentFailure,
    }
}
