use pod0_application::{
    CoreWakeReason, FEED_FETCH_HOST_REQUEST_DEADLINE_MILLISECONDS, FeedFetchIntent,
    FeedFetchProjection, FeedFetchStage, HostRequest, HostRequestEnvelope,
    MAX_ACTIVE_FEED_FETCH_WORKFLOWS, MAX_FEED_FETCH_ATTEMPTS, MAX_FEED_RESPONSE_BYTES,
};
use pod0_domain::{HostRequestId, UnixTimestampMilliseconds};
use pod0_storage::{
    FeedFetchFailureInput, FeedFetchWorkflowRecord, StoredFeedFetchIntent, StoredFeedFetchStage,
};

use crate::runtime_state::FacadeState;

impl FacadeState {
    pub(super) fn rehydrate_feed_workflows(&mut self) -> Result<(), pod0_storage::StorageError> {
        self.reload_feed_fetches()?;
        self.admit_feed_fetch_requests()
    }

    pub(super) fn reload_feed_fetches(&mut self) -> Result<(), pod0_storage::StorageError> {
        self.feed_fetches = self
            .store
            .as_ref()
            .map(|store| store.feed_fetch_workflows_snapshot(MAX_ACTIVE_FEED_FETCH_WORKFLOWS))
            .transpose()?
            .unwrap_or_default();
        Ok(())
    }

    pub(super) fn admit_feed_fetch_requests(&mut self) -> Result<(), pod0_storage::StorageError> {
        let records = self
            .store
            .as_ref()
            .map(|store| store.active_feed_fetch_workflows(MAX_ACTIVE_FEED_FETCH_WORKFLOWS))
            .transpose()?
            .unwrap_or_default();
        for record in records {
            let _ = self.queue_feed_fetch_request(record);
        }
        Ok(())
    }

    pub(super) fn queue_feed_fetch_request(&mut self, record: FeedFetchWorkflowRecord) -> bool {
        if self.pending_feeds.contains_key(&record.request_id) {
            return true;
        }
        if record.stage == StoredFeedFetchStage::RetryScheduled
            && record
                .not_before_ms
                .is_some_and(|not_before| not_before > self.now().value)
        {
            self.schedule_feed_fetch_retry_wake(&record);
            return false;
        }
        let envelope = feed_host_request(&record);
        if !self.host_requests.register(envelope.clone())
            && !self.host_requests.matches_outstanding(&envelope)
        {
            return false;
        }
        if !self
            .host_queue
            .iter()
            .any(|queued| queued.request_id == record.request_id)
        {
            self.host_queue.push_back(envelope);
        }
        self.pending_feeds.insert(record.request_id, record);
        true
    }

    pub(super) fn withdraw_feed_fetch_request(&mut self, request_id: HostRequestId) {
        let was_queued = self
            .host_queue
            .iter()
            .any(|request| request.request_id == request_id);
        self.host_queue
            .retain(|request| request.request_id != request_id);
        let pending = self.pending_feeds.remove(&request_id);
        if self.host_requests.cancel_request(request_id)
            && !was_queued
            && let Some(record) = pending
        {
            self.host_cancellations
                .push_back(pod0_application::HostCancellationRequest {
                    request_id,
                    cancellation_id: record.cancellation_id,
                });
        }
        self.host_requests.retire(request_id);
    }

    /// Rust owns fetch expiry: requests whose deadline elapsed are withdrawn
    /// and rescheduled here, never expired by native code (ADR-0001).
    pub(super) fn reconcile_feed_fetch_deadlines(&mut self) -> bool {
        let Some(store) = self.store.clone() else {
            return false;
        };
        let now = self.now().value;
        let records = match store.active_feed_fetch_workflows(MAX_ACTIVE_FEED_FETCH_WORKFLOWS) {
            Ok(value) => value,
            Err(_) => return false,
        };
        let mut changed = false;
        for record in records.into_iter().filter(|record| {
            record.stage == StoredFeedFetchStage::Requested
                && record
                    .deadline_at_ms
                    .is_some_and(|deadline| deadline <= now)
        }) {
            self.withdraw_feed_fetch_request(record.request_id);
            if self
                .schedule_feed_fetch_failure(&record, "timed_out", true)
                .is_some()
            {
                changed = true;
            }
        }
        if changed {
            let _ = self.reload_feed_fetches();
        }
        changed
    }

    /// Applies the kernel retry policy to a failed attempt. Returns the
    /// updated record when the workflow survived (scheduled or parked).
    pub(super) fn schedule_feed_fetch_failure(
        &mut self,
        record: &FeedFetchWorkflowRecord,
        failure_code: &str,
        retryable: bool,
    ) -> Option<FeedFetchWorkflowRecord> {
        let store = self.store.clone()?;
        let now = self.now();
        let retry = retryable && record.attempt < MAX_FEED_FETCH_ATTEMPTS;
        let retry_at = retry
            .then(|| pod0_application::feed_fetch_retry_not_before(now, record.attempt).value());
        let retry_deadline = retry_at
            .and_then(|value| value.checked_add(FEED_FETCH_HOST_REQUEST_DEADLINE_MILLISECONDS));
        let updated = store
            .fail_feed_fetch_workflow(FeedFetchFailureInput {
                request_id: record.request_id,
                failure_code: failure_code.to_owned(),
                retryable: retry,
                retry_at_ms: retry_at,
                retry_deadline_at_ms: retry_deadline,
                issued_revision: self.revision,
                observed_at_ms: now.value,
            })
            .ok()
            .flatten()?;
        if updated.stage == StoredFeedFetchStage::RetryScheduled {
            self.schedule_feed_fetch_retry_wake(&updated);
        }
        Some(updated)
    }

    pub(super) fn schedule_feed_fetch_retry_wake(
        &mut self,
        record: &FeedFetchWorkflowRecord,
    ) -> bool {
        let Some(wake_at) = record.not_before_ms else {
            return false;
        };
        self.schedule_core_wake(
            record.command_id,
            record.cancellation_id,
            record.issued_revision,
            wake_at,
            CoreWakeReason::FeedFetchRetry {
                podcast_id: record.podcast_id,
                attempt: record.attempt,
            },
        )
    }

    pub(super) fn feed_fetch_projection(&self) -> Vec<FeedFetchProjection> {
        self.feed_fetches
            .iter()
            .map(|record| FeedFetchProjection {
                podcast_id: record.podcast_id,
                feed_url: record.source_url.clone(),
                intent: match record.intent {
                    StoredFeedFetchIntent::Subscribe => FeedFetchIntent::Subscribe,
                    StoredFeedFetchIntent::Ensure => FeedFetchIntent::Ensure,
                    StoredFeedFetchIntent::Refresh => FeedFetchIntent::Refresh,
                    StoredFeedFetchIntent::Metadata => FeedFetchIntent::Metadata,
                },
                stage: match record.stage {
                    StoredFeedFetchStage::Requested => FeedFetchStage::Requested,
                    StoredFeedFetchStage::RetryScheduled => FeedFetchStage::RetryScheduled,
                    StoredFeedFetchStage::Failed => FeedFetchStage::Failed,
                },
                attempt: record.attempt,
                request_id: record.request_id,
                not_before: record.not_before_ms.map(UnixTimestampMilliseconds::new),
                failure_code: record.failure_code.clone(),
                updated_at: UnixTimestampMilliseconds::new(record.updated_at_ms),
            })
            .collect()
    }
}

fn feed_host_request(record: &FeedFetchWorkflowRecord) -> HostRequestEnvelope {
    HostRequestEnvelope {
        request_id: record.request_id,
        command_id: record.command_id,
        cancellation_id: record.cancellation_id,
        issued_revision: record.issued_revision,
        deadline_at: record.deadline_at_ms.map(UnixTimestampMilliseconds::new),
        request: HostRequest::FetchFeed {
            feed_url: record.source_url.clone(),
            entity_tag: record.entity_tag.clone(),
            last_modified: record.last_modified.clone(),
            maximum_response_bytes: MAX_FEED_RESPONSE_BYTES,
        },
    }
}
