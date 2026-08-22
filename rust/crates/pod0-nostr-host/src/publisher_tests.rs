use std::{
    thread,
    time::{Duration, Instant},
};

use pod0_application::{LeasedNMPPublicationDraft, Pod0PublicationDraft};
use pod0_domain::{
    ActivityCorrelationId, ActivityId, EffectAttemptId, EffectIntentId, EffectLeaseId,
    PublicationId, UnixTimestampMilliseconds,
};

use super::*;

const SECRET: &str = "0000000000000000000000000000000000000000000000000000000000000001";
const AUTHOR: &str = "79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";

#[test]
fn cancellation_before_signing_has_no_handoff_or_receipt() {
    let publisher = NostrPublisher::new(
        ["ws://127.0.0.1:9"],
        SigningSecret::parse(SECRET).unwrap(),
        PublisherConfig::default(),
    )
    .unwrap();
    let cancellation = CancellationToken::new();
    cancellation.cancel();
    let error = publisher.publish(fixture(), &cancellation).unwrap_err();
    let PublishError::Cancelled(failure) = error else {
        panic!("expected cancellation")
    };
    assert!(failure.event_id_hex.is_none());
    assert!(!failure.handoff_possible());
    assert!(failure.relay_outcomes.is_empty());
}

#[test]
fn new_with_handle_publish_completes_when_driven_from_a_second_thread() {
    // A multi-thread runtime built on this (the test) thread; its `Handle` is
    // then used to drive `publish` from a *different*, spawned OS thread. A
    // current-thread runtime's `Handle::block_on` would hang forever here —
    // this is the exact pitfall 01-02-SUMMARY.md documented and fixed for
    // pod0-cli/pod0-portable-media.
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .unwrap();
    let handle = runtime.handle().clone();

    let started = Instant::now();
    let result = thread::spawn(move || {
        let config = PublisherConfig {
            operation_timeout: Duration::from_secs(5),
            ..PublisherConfig::default()
        };
        let publisher = NostrPublisher::new_with_handle(
            ["ws://127.0.0.1:9"],
            SigningSecret::parse(SECRET).unwrap(),
            config,
            handle,
        )
        .unwrap();
        let cancellation = CancellationToken::new();
        // Not pre-cancelled: this must genuinely reach the `block_on` call
        // site inside `publish` (via `publish_to_relay`'s connection attempt
        // to an unreachable local port) rather than short-circuit before it.
        publisher.publish(fixture(), &cancellation)
    })
    .join()
    .expect("publish thread must not panic");

    assert!(
        started.elapsed() < Duration::from_secs(10),
        "publish did not return within the bounded wall-clock deadline — Handle::block_on likely hung"
    );
    // The assertion under test is "this returns at all" — port 9 has no relay
    // listening, so the specific error variant (connection failure surfaced
    // as a relay outcome, then AcknowledgementThresholdNotMet) is incidental.
    assert!(result.is_err());
}

#[test]
fn secret_debug_is_redacted() {
    let secret = SigningSecret::parse(SECRET).unwrap();
    let debug = format!("{secret:?}");
    assert!(!debug.contains(SECRET));
    assert!(debug.contains(AUTHOR));
}

#[test]
fn dependency_frame_logging_is_statically_disabled() {
    assert!(log::STATIC_MAX_LEVEL <= log::LevelFilter::Info);
}

fn fixture() -> LeasedNMPPublicationDraft {
    LeasedNMPPublicationDraft {
        lease: PersistedEffectLeaseIdentity {
            intent_id: EffectIntentId::from_parts(1, 1),
            authorizing_activity_id: ActivityId::from_parts(2, 2),
            correlation_id: ActivityCorrelationId::from_parts(3, 3),
            attempt_id: EffectAttemptId::from_parts(4, 4),
            lease_id: EffectLeaseId::from_parts(5, 5),
            fence: 1,
            expires_at: UnixTimestampMilliseconds::new(unix_milliseconds() + 60_000),
        },
        draft: Pod0PublicationDraft {
            publication_id: PublicationId::from_parts(6, 6),
            expected_author_hex: AUTHOR.into(),
            correlation_token: "correlation".into(),
            created_at_seconds: 1,
            kind: 1,
            tags: vec![],
            content: "payload".into(),
        },
    }
}
