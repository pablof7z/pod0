use pod0_domain::{HostRequestId, PodcastId, UnixTimestampMilliseconds};

/// Feed fetches are re-issued on relaunch, so the deadline is a scheduling
/// boundary for kernel reconciliation, not a native-owned expiry gate.
pub const FEED_FETCH_HOST_REQUEST_DEADLINE_MILLISECONDS: i64 = 24 * 60 * 60 * 1_000;
pub const FEED_FETCH_RETRY_BASE_MILLISECONDS: i64 = 60 * 1_000;
pub const FEED_FETCH_RETRY_MAX_MILLISECONDS: i64 = 6 * 60 * 60 * 1_000;
/// Terminal cap: past this attempt count a transient failure stops retrying.
pub const MAX_FEED_FETCH_ATTEMPTS: u16 = 8;
pub const MAX_ACTIVE_FEED_FETCH_WORKFLOWS: u16 = 200;

/// Computes the kernel-owned retry boundary for a failed feed-fetch attempt.
/// Native hosts schedule the returned instant but never choose retry timing.
/// The delay doubles per failed attempt from one minute up to six hours.
#[must_use]
pub fn feed_fetch_retry_not_before(
    observed_at: UnixTimestampMilliseconds,
    failed_attempt: u16,
) -> UnixTimestampMilliseconds {
    let exponent = u32::from(failed_attempt.saturating_sub(1).min(16));
    let delay = FEED_FETCH_RETRY_BASE_MILLISECONDS
        .saturating_mul(1_i64 << exponent)
        .min(FEED_FETCH_RETRY_MAX_MILLISECONDS);
    UnixTimestampMilliseconds::new(observed_at.value().saturating_add(delay))
}

/// Kernel-owned classification of host fetch failures: transient transport
/// conditions schedule a durable retry; everything else parks the workflow.
#[must_use]
pub const fn feed_fetch_failure_is_retryable(code: crate::HostFailureCode) -> bool {
    matches!(
        code,
        crate::HostFailureCode::Offline
            | crate::HostFailureCode::TimedOut
            | crate::HostFailureCode::ProviderUnavailable
            | crate::HostFailureCode::PlatformFailure
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum FeedFetchIntent {
    Subscribe,
    Ensure,
    Refresh,
    Metadata,
    Unsupported { wire_code: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum FeedFetchStage {
    Requested,
    RetryScheduled,
    Failed,
    Unsupported { wire_code: u32 },
}

/// Durable progress of one feed-fetch workflow, projected so the UI renders
/// "Subscribing…" from state that survives relaunch instead of from a blocked
/// continuation or per-row native spinner state.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct FeedFetchProjection {
    pub podcast_id: PodcastId,
    pub feed_url: String,
    pub intent: FeedFetchIntent,
    pub stage: FeedFetchStage,
    pub attempt: u16,
    pub request_id: HostRequestId,
    pub not_before: Option<UnixTimestampMilliseconds>,
    pub failure_code: Option<String>,
    pub updated_at: UnixTimestampMilliseconds,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_backoff_doubles_from_one_minute_and_caps_at_six_hours() {
        let observed = UnixTimestampMilliseconds::new(1_000_000);
        assert_eq!(
            feed_fetch_retry_not_before(observed, 1).value(),
            1_000_000 + 60_000
        );
        assert_eq!(
            feed_fetch_retry_not_before(observed, 2).value(),
            1_000_000 + 120_000
        );
        assert_eq!(
            feed_fetch_retry_not_before(observed, 3).value(),
            1_000_000 + 240_000
        );
        assert_eq!(
            feed_fetch_retry_not_before(observed, 16).value(),
            1_000_000 + FEED_FETCH_RETRY_MAX_MILLISECONDS
        );
        assert_eq!(
            feed_fetch_retry_not_before(observed, u16::MAX).value(),
            1_000_000 + FEED_FETCH_RETRY_MAX_MILLISECONDS
        );
    }

    #[test]
    fn retry_backoff_saturates_instead_of_overflowing() {
        let observed = UnixTimestampMilliseconds::new(i64::MAX - 1);
        assert_eq!(feed_fetch_retry_not_before(observed, 1).value(), i64::MAX);
    }
}
