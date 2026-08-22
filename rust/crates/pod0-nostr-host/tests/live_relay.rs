use std::time::{SystemTime, UNIX_EPOCH};

use pod0_application::{
    LeasedNMPPublicationDraft, PersistedEffectLeaseIdentity, Pod0PublicationDraft,
};
use pod0_domain::{
    ActivityCorrelationId, ActivityId, EffectAttemptId, EffectIntentId, EffectLeaseId,
    PublicationId, UnixTimestampMilliseconds,
};
use pod0_nostr_host::{
    AcknowledgementThreshold, CancellationToken, NostrPublisher, PublisherConfig, SigningSecret,
};

#[test]
#[ignore = "requires POD0_NOSTR_LIVE_SECRET and POD0_NOSTR_LIVE_RELAYS"]
fn publishes_to_live_relays_only_after_real_ok_acknowledgements() {
    let secret_value =
        std::env::var("POD0_NOSTR_LIVE_SECRET").expect("live signing secret is required");
    let relay_value =
        std::env::var("POD0_NOSTR_LIVE_RELAYS").expect("live relay URLs are required");
    let relays = relay_value
        .split(',')
        .map(str::trim)
        .filter(|relay| !relay.is_empty())
        .collect::<Vec<_>>();
    let required = std::env::var("POD0_NOSTR_LIVE_ACKS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(relays.len());
    let secret = SigningSecret::parse(&secret_value).expect("valid live signing secret");
    let author = secret.public_key_hex();
    let publisher = NostrPublisher::new(
        relays,
        secret,
        PublisherConfig {
            acknowledgement_threshold: AcknowledgementThreshold::AtLeast(required),
            ..PublisherConfig::default()
        },
    )
    .expect("valid live publisher configuration");
    let now_ms = unix_milliseconds();
    let evidence = publisher
        .publish(
            LeasedNMPPublicationDraft {
                lease: PersistedEffectLeaseIdentity {
                    intent_id: EffectIntentId::from_parts(1, 1),
                    authorizing_activity_id: ActivityId::from_parts(2, 2),
                    correlation_id: ActivityCorrelationId::from_parts(3, 3),
                    attempt_id: EffectAttemptId::from_parts(4, 4),
                    lease_id: EffectLeaseId::from_parts(5, 5),
                    fence: 1,
                    expires_at: UnixTimestampMilliseconds::new(now_ms + 120_000),
                },
                draft: Pod0PublicationDraft {
                    publication_id: PublicationId::from_parts(6, now_ms as u64),
                    expected_author_hex: author,
                    correlation_token: format!("live-{now_ms}"),
                    created_at_seconds: (now_ms / 1_000) as u64,
                    kind: 20_001,
                    tags: vec![vec!["client".into(), "pod0-nostr-host".into()]],
                    content: format!("pod0-nostr-host live verification {now_ms}"),
                },
            },
            &CancellationToken::new(),
        )
        .expect("live relays must return actual matching OK acknowledgements");

    assert_eq!(evidence.event_id_hex.len(), 64);
    assert!(evidence.acknowledgement_count() >= required);
}

fn unix_milliseconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| i64::try_from(duration.as_millis()).ok())
        .expect("system clock after Unix epoch")
}
