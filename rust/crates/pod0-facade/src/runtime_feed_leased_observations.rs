use pod0_application::{
    DurableFeedHostObservation, DurableFeedObservationOutcome, HostObservationReceipt,
    HostObservationRejection, LeasedHostObservationEnvelope, feed_fetch_failure_is_retryable,
    feed_fetch_retry_not_before,
};
use pod0_storage::{FeedFetchLeasedObservationAction, FeedFetchObservationCommitInput};

use crate::runtime_chapter_model_receipts::{persisted, retain};
use crate::runtime_leased_observations::rejected_payload;
use crate::runtime_state::FacadeState;

impl FacadeState {
    pub(super) fn record_leased_feed_observation(
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
            .active_feed_fetch_workflows(pod0_application::MAX_ACTIVE_FEED_FETCH_WORKFLOWS)
            .map(|records| {
                records
                    .into_iter()
                    .find(|record| record.request_id == request_id)
            })
        else {
            return (
                false,
                rejected_payload(request_id, HostObservationRejection::StaleWorkflow),
            );
        };
        let action = match &observation.outcome {
            DurableFeedObservationOutcome::Fetched {
                bytes,
                entity_tag,
                last_modified,
                ..
            } => {
                let identity = pod0_domain::FeedIdentityV1 {
                    source_url: record.source_url.clone(),
                    comparison_key: record.feed_key.clone(),
                };
                let Ok(parsed) = pod0_application::parse_podcast_feed(
                    bytes,
                    identity,
                    record.podcast_id,
                    observation.observed_at,
                ) else {
                    return self.commit_failed_feed_observation(
                        leased,
                        observation,
                        record,
                        "feed_malformed",
                    );
                };
                FeedFetchLeasedObservationAction::Apply {
                    parsed,
                    entity_tag: entity_tag.clone(),
                    last_modified: last_modified.clone(),
                }
            }
            DurableFeedObservationOutcome::NotModified {
                entity_tag,
                last_modified,
                ..
            } => FeedFetchLeasedObservationAction::NotModified {
                entity_tag: entity_tag.clone(),
                last_modified: last_modified.clone(),
            },
            DurableFeedObservationOutcome::Failed { code } => {
                let retry = feed_fetch_failure_is_retryable(*code)
                    && record.attempt < pod0_application::MAX_FEED_FETCH_ATTEMPTS;
                let retry_at = retry.then(|| {
                    feed_fetch_retry_not_before(observation.observed_at, record.attempt).value
                });
                FeedFetchLeasedObservationAction::Fail {
                    failure_code: feed_failure_code_text(*code).to_owned(),
                    retry_at_ms: retry_at,
                    retry_deadline_at_ms: retry_at.and_then(|value| {
                        value.checked_add(
                            pod0_application::FEED_FETCH_HOST_REQUEST_DEADLINE_MILLISECONDS,
                        )
                    }),
                }
            }
            DurableFeedObservationOutcome::Cancelled => FeedFetchLeasedObservationAction::Cancel,
            DurableFeedObservationOutcome::NotificationDelivered { .. } => {
                return (
                    false,
                    rejected_payload(request_id, HostObservationRejection::MismatchedPayload),
                );
            }
        };
        self.commit_leased_feed_observation(leased, observation, action)
    }

    fn commit_failed_feed_observation(
        &mut self,
        leased: LeasedHostObservationEnvelope,
        observation: DurableFeedHostObservation,
        _record: pod0_storage::FeedFetchWorkflowRecord,
        failure_code: &str,
    ) -> (bool, HostObservationReceipt) {
        self.commit_leased_feed_observation(
            leased,
            observation,
            FeedFetchLeasedObservationAction::Fail {
                failure_code: failure_code.to_owned(),
                retry_at_ms: None,
                retry_deadline_at_ms: None,
            },
        )
    }

    fn commit_leased_feed_observation(
        &mut self,
        leased: LeasedHostObservationEnvelope,
        observation: DurableFeedHostObservation,
        action: FeedFetchLeasedObservationAction,
    ) -> (bool, HostObservationReceipt) {
        let request_id = observation.request_id;
        let Some(store) = self.store.clone() else {
            return (false, retain(request_id));
        };
        let result = store.commit_feed_fetch_observation(FeedFetchObservationCommitInput {
            lease: leased.lease,
            observation,
            action,
            committed_at: self.now(),
        });
        let outcome = match result {
            Ok(value) => value,
            Err(pod0_storage::StorageError::CommandConflict) => {
                return (
                    false,
                    rejected_payload(request_id, HostObservationRejection::StaleWorkflow),
                );
            }
            Err(_) => return (false, retain(request_id)),
        };
        if outcome.replayed {
            return (
                false,
                rejected_payload(request_id, HostObservationRejection::Duplicate),
            );
        }
        let _ = self.reload_listening();
        let _ = self.reload_feed_fetches();
        let _ = self.reconcile_feed_discovery_workflows();
        self.advance_revision();
        (true, persisted(request_id, true))
    }
}

fn feed_failure_code_text(code: pod0_application::HostFailureCode) -> &'static str {
    match code {
        pod0_application::HostFailureCode::Offline => "offline",
        pod0_application::HostFailureCode::TimedOut => "timed_out",
        pod0_application::HostFailureCode::PermissionDenied => "permission_denied",
        pod0_application::HostFailureCode::InvalidResponse => "invalid_response",
        pod0_application::HostFailureCode::ResponseTooLarge => "response_too_large",
        pod0_application::HostFailureCode::MediaUnavailable => "media_unavailable",
        pod0_application::HostFailureCode::ProviderUnavailable => "provider_unavailable",
        pod0_application::HostFailureCode::Unauthorized => "unauthorized",
        pod0_application::HostFailureCode::IndexUnavailable => "index_unavailable",
        pod0_application::HostFailureCode::PlatformFailure => "platform_failure",
        pod0_application::HostFailureCode::Unsupported { .. } => "unsupported",
    }
}
