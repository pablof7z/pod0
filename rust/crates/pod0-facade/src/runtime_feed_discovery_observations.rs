use pod0_application::{
    HostCancellationRequest, HostFailureCode, HostObservation, HostObservationEnvelope,
    HostObservationReceipt, HostObservationRejection,
};
use pod0_storage::{FeedDiscoveryEffectStage, FeedDiscoveryNotificationOutcome};

use crate::runtime_observation_mapping::retain;
use crate::runtime_state::FacadeState;

impl FacadeState {
    pub(super) fn persist_feed_discovery_notification_observation(
        &mut self,
        envelope: HostObservationEnvelope,
    ) -> (bool, HostObservationReceipt) {
        let request_id = envelope.request_id;
        let Some(pending) = self
            .pending_feed_discovery_notifications
            .get(&request_id)
            .cloned()
        else {
            return (
                false,
                reject_reason(request_id, HostObservationRejection::UnknownRequest),
            );
        };
        let outcome = match envelope.observation {
            HostObservation::NewEpisodeNotificationDelivered {
                occurrence_id,
                episode_id,
            } if occurrence_id == pending.occurrence_id && episode_id == pending.episode_id => {
                FeedDiscoveryNotificationOutcome::Delivered
            }
            HostObservation::NewEpisodeNotificationDelivered { .. } => {
                return (
                    false,
                    reject_reason(request_id, HostObservationRejection::MismatchedPayload),
                );
            }
            HostObservation::Failed { code, .. } => match code {
                HostFailureCode::Offline
                | HostFailureCode::TimedOut
                | HostFailureCode::ProviderUnavailable
                | HostFailureCode::PlatformFailure => {
                    FeedDiscoveryNotificationOutcome::RetryableFailure
                }
                HostFailureCode::PermissionDenied | HostFailureCode::Unauthorized => {
                    FeedDiscoveryNotificationOutcome::PermissionDenied
                }
                HostFailureCode::InvalidResponse
                | HostFailureCode::ResponseTooLarge
                | HostFailureCode::MediaUnavailable
                | HostFailureCode::IndexUnavailable
                | HostFailureCode::Unsupported { .. } => {
                    FeedDiscoveryNotificationOutcome::PermanentFailure
                }
            },
            HostObservation::Cancelled => FeedDiscoveryNotificationOutcome::Cancelled,
            _ => {
                return (
                    false,
                    reject_reason(request_id, HostObservationRejection::MismatchedPayload),
                );
            }
        };
        let Some(store) = self.store.clone() else {
            return (false, retain(request_id));
        };
        match store.finish_feed_discovery_notification(
            request_id,
            outcome,
            envelope.observed_at.value,
        ) {
            Ok(Some(record)) => {
                self.retire_feed_discovery_notification(request_id);
                if record.stage == FeedDiscoveryEffectStage::RetryScheduled {
                    self.schedule_feed_discovery_retry(&record);
                }
                (
                    true,
                    HostObservationReceipt::Persisted {
                        request_id,
                        terminal: true,
                    },
                )
            }
            Ok(None) => (
                false,
                reject_reason(request_id, HostObservationRejection::UnknownRequest),
            ),
            Err(_) => (false, retain(request_id)),
        }
    }

    fn retire_feed_discovery_notification(&mut self, request_id: pod0_domain::HostRequestId) {
        self.pending_feed_discovery_notifications
            .remove(&request_id);
        self.pending_feed_discovery_notification_observations
            .remove(&request_id);
        self.host_requests.retire(request_id);
    }

    pub(super) fn withdraw_feed_discovery_notification(
        &mut self,
        request_id: pod0_domain::HostRequestId,
    ) {
        let was_queued = self
            .host_queue
            .iter()
            .any(|request| request.request_id == request_id);
        self.host_queue
            .retain(|request| request.request_id != request_id);
        let pending = self
            .pending_feed_discovery_notifications
            .remove(&request_id);
        self.pending_feed_discovery_notification_observations
            .remove(&request_id);
        if self.host_requests.cancel_request(request_id)
            && !was_queued
            && let Some(record) = pending
        {
            self.host_cancellations.push_back(HostCancellationRequest {
                request_id,
                cancellation_id: record.cancellation_id,
            });
        }
        self.host_requests.retire(request_id);
    }
}

fn reject_reason(
    request_id: pod0_domain::HostRequestId,
    reason: HostObservationRejection,
) -> HostObservationReceipt {
    HostObservationReceipt::Rejected { request_id, reason }
}
