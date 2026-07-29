use pod0_application::{
    HostObservation, HostObservationEnvelope, HostObservationReceipt,
    feed_fetch_failure_is_retryable,
};
use pod0_storage::{FeedFetchWorkflowRecord, StoredFeedFetchIntent};

use crate::runtime_state::FacadeState;

impl FacadeState {
    /// Applies a host observation to the durable feed-fetch workflow. Every
    /// returned `Persisted` receipt corresponds to a committed transition:
    /// the parsed feed applied, a retry scheduled, or the workflow parked.
    /// The commanding operation finished at dispatch time and is never
    /// touched here.
    pub(super) fn persist_feed_observation(
        &mut self,
        record: FeedFetchWorkflowRecord,
        observation: HostObservationEnvelope,
    ) -> HostObservationReceipt {
        let request_id = observation.request_id;
        self.advance_revision();
        self.pending_feeds.remove(&request_id);
        let observed_at_ms = observation.observed_at.value;
        let receipt = match observation.observation {
            HostObservation::FeedBytesFetched {
                bytes,
                entity_tag,
                last_modified,
                ..
            } => self.apply_fetched_feed(&record, &bytes, entity_tag, last_modified, observed_at_ms),
            HostObservation::FeedNotModified {
                entity_tag,
                last_modified,
                ..
            } => self.apply_not_modified(&record, entity_tag, last_modified, observed_at_ms),
            HostObservation::Failed { code, .. } => self.fail_feed_fetch(
                &record,
                feed_failure_code_text(code),
                feed_fetch_failure_is_retryable(code),
            ),
            HostObservation::Cancelled => self.discard_feed_fetch(&record),
            _ => self.fail_feed_fetch(&record, "invalid_observation", false),
        };
        let _ = self.reload_feed_fetches();
        self.trim_operations();
        receipt
    }

    /// A response the ledger bounds rejected still resolves the workflow
    /// durably instead of leaving the fetch outstanding forever.
    pub(super) fn persist_oversized_feed_observation(
        &mut self,
        record: FeedFetchWorkflowRecord,
    ) -> HostObservationReceipt {
        self.advance_revision();
        self.pending_feeds.remove(&record.request_id);
        let receipt = self.fail_feed_fetch(&record, "response_too_large", false);
        let _ = self.reload_feed_fetches();
        self.trim_operations();
        receipt
    }

    fn apply_fetched_feed(
        &mut self,
        record: &FeedFetchWorkflowRecord,
        bytes: &[u8],
        entity_tag: Option<String>,
        last_modified: Option<String>,
        observed_at_ms: i64,
    ) -> HostObservationReceipt {
        let identity = pod0_domain::FeedIdentityV1 {
            source_url: record.source_url.clone(),
            comparison_key: record.feed_key.clone(),
        };
        let parsed = pod0_application::parse_podcast_feed(
            bytes,
            identity,
            record.podcast_id,
            pod0_domain::UnixTimestampMilliseconds::new(observed_at_ms),
        );
        let Ok(parsed) = parsed else {
            return self.fail_feed_fetch(record, "feed_malformed", false);
        };
        let Some(store) = self.store.clone() else {
            return retain(record.request_id);
        };
        let mut episodes = parsed.episodes;
        if record.intent == StoredFeedFetchIntent::Metadata {
            episodes.clear();
        }
        let result = store.apply_feed(
            record.command_id,
            &record.command_fingerprint,
            parsed.podcast,
            episodes,
            record.intent == StoredFeedFetchIntent::Subscribe,
            record.intent == StoredFeedFetchIntent::Refresh,
            entity_tag,
            last_modified,
            observed_at_ms,
        );
        match result {
            Ok(_) => {
                let _ = store.complete_feed_fetch_workflow(record.request_id);
                let _ = self.reload_listening();
                let _ = self.reconcile_feed_discovery_workflows();
                self.host_requests.retire(record.request_id);
                persisted(record.request_id)
            }
            Err(_) => self.fail_feed_fetch(record, "storage_unavailable", true),
        }
    }

    fn apply_not_modified(
        &mut self,
        record: &FeedFetchWorkflowRecord,
        entity_tag: Option<String>,
        last_modified: Option<String>,
        observed_at_ms: i64,
    ) -> HostObservationReceipt {
        let Some(store) = self.store.clone() else {
            return retain(record.request_id);
        };
        let result = if matches!(
            record.intent,
            StoredFeedFetchIntent::Refresh | StoredFeedFetchIntent::Metadata
        ) {
            store
                .mark_feed_not_modified(
                    record.command_id,
                    &record.command_fingerprint,
                    record.podcast_id,
                    entity_tag,
                    last_modified,
                    observed_at_ms,
                )
                .map(|_| ())
        } else {
            Ok(())
        };
        match result {
            Ok(()) => {
                let _ = store.complete_feed_fetch_workflow(record.request_id);
                let _ = self.reload_listening();
                self.host_requests.retire(record.request_id);
                persisted(record.request_id)
            }
            Err(_) => self.fail_feed_fetch(record, "storage_unavailable", true),
        }
    }

    fn fail_feed_fetch(
        &mut self,
        record: &FeedFetchWorkflowRecord,
        failure_code: &str,
        retryable: bool,
    ) -> HostObservationReceipt {
        self.host_requests.retire(record.request_id);
        match self.schedule_feed_fetch_failure(record, failure_code, retryable) {
            Some(_) => persisted(record.request_id),
            None => retain(record.request_id),
        }
    }

    fn discard_feed_fetch(&mut self, record: &FeedFetchWorkflowRecord) -> HostObservationReceipt {
        self.host_requests.retire(record.request_id);
        match self
            .store
            .as_ref()
            .map(|store| store.complete_feed_fetch_workflow(record.request_id))
        {
            Some(Ok(_)) => persisted(record.request_id),
            _ => retain(record.request_id),
        }
    }
}

fn persisted(request_id: pod0_domain::HostRequestId) -> HostObservationReceipt {
    HostObservationReceipt::Persisted {
        request_id,
        terminal: true,
    }
}

fn retain(request_id: pod0_domain::HostRequestId) -> HostObservationReceipt {
    HostObservationReceipt::RetainAndRetry { request_id }
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
