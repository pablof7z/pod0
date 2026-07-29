use std::collections::BTreeMap;

use pod0_domain::UnixTimestampMilliseconds;

use crate::{
    FACADE_CONTRACT_VERSION, FEED_FETCH_HOST_REQUEST_DEADLINE_MILLISECONDS,
    FEED_FETCH_RETRY_BASE_MILLISECONDS, FEED_FETCH_RETRY_MAX_MILLISECONDS, MAX_FEED_FETCH_ATTEMPTS,
    feed_fetch_retry_not_before,
};

fn values() -> BTreeMap<&'static str, &'static str> {
    include_str!("../../../../Fixtures/CoreKnowledge/feed-fetch-contract-v1.properties")
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| line.split_once('=').expect("valid golden property"))
        .collect()
}

fn number<T: std::str::FromStr>(values: &BTreeMap<&str, &str>, key: &str) -> T {
    values[key]
        .parse()
        .unwrap_or_else(|_| panic!("valid {key}"))
}

/// The v53 compatibility fixture: `Succeeded` for the feed family means
/// "durably queued", and the kernel retry policy is pinned cross-platform.
#[test]
fn rust_matches_the_cross_platform_feed_fetch_fixture() {
    let values = values();
    assert_eq!(number::<u32>(&values, "fixture_version"), 1);
    assert_eq!(
        number::<u32>(&values, "contract_version"),
        FACADE_CONTRACT_VERSION
    );
    assert_eq!(values["subscribe_succeeded_meaning"], "durably-queued");
    assert_eq!(values["unknown_future_field"], "ignored-by-v1-readers");
    assert_eq!(
        number::<i64>(&values, "retry_base_milliseconds"),
        FEED_FETCH_RETRY_BASE_MILLISECONDS
    );
    assert_eq!(
        number::<i64>(&values, "retry_max_milliseconds"),
        FEED_FETCH_RETRY_MAX_MILLISECONDS
    );
    assert_eq!(
        number::<u16>(&values, "max_feed_fetch_attempts"),
        MAX_FEED_FETCH_ATTEMPTS
    );
    assert_eq!(
        number::<i64>(&values, "host_request_deadline_milliseconds"),
        FEED_FETCH_HOST_REQUEST_DEADLINE_MILLISECONDS
    );
    let observed =
        UnixTimestampMilliseconds::new(number(&values, "retry_observed_at_milliseconds"));
    for (attempt, key) in [
        (1, "retry_not_before_attempt_1"),
        (2, "retry_not_before_attempt_2"),
        (10, "retry_not_before_attempt_10"),
    ] {
        assert_eq!(
            feed_fetch_retry_not_before(observed, attempt).value(),
            number::<i64>(&values, key),
            "retry boundary for attempt {attempt}"
        );
    }
}
