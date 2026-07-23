use std::collections::BTreeSet;
use std::sync::mpsc;
use std::time::Duration;

use pod0_application::Pod0PublicationDraft;
use pod0_domain::{PublicationFactKind, PublicationId};

use super::{NmpRuntime, NmpRuntimeConfig, PublicationReattachment, WriteStatus, observations};

mod scripted_relay;
use scripted_relay::{BoundRelay, relay_list_event};

fn draft() -> Pod0PublicationDraft {
    Pod0PublicationDraft {
        publication_id: PublicationId::from_parts(1, 2),
        expected_author_hex: "79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798"
            .into(),
        correlation_token: "pod0-pub-v1:00000000000000010000000000000002".into(),
        created_at_seconds: 1_800_000_000,
        kind: 30_075,
        tags: vec![vec!["d".into(), "podcast:item:guid:test".into()]],
        content: "test".into(),
    }
}

fn config(path: String) -> NmpRuntimeConfig {
    NmpRuntimeConfig {
        store_path: path,
        indexer_relays: Vec::new(),
        app_relays: Vec::new(),
        fallback_relays: Vec::new(),
        allowed_local_relay_hosts: Vec::new(),
    }
}

#[test]
fn missing_signer_retains_expected_author_and_receipt_replays_after_restart() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("nmp.redb");
    let (sender, receiver) = mpsc::channel();
    let runtime = NmpRuntime::start(config(path.to_string_lossy().into()), sender).unwrap();
    let pending = runtime.publish_tracked(&draft()).unwrap();
    let receipt_id = pending.receipt_id;
    runtime.attach(pending);
    let accepted = receiver.recv_timeout(Duration::from_secs(2)).unwrap();
    let awaiting = receiver.recv_timeout(Duration::from_secs(2)).unwrap();
    assert_eq!(accepted.observation.kind, PublicationFactKind::Accepted);
    assert_eq!(
        awaiting.observation.kind,
        PublicationFactKind::AwaitingCapability
    );
    let expected_author = draft().expected_author_hex;
    assert_eq!(
        awaiting.observation.detail.as_deref(),
        Some(expected_author.as_str())
    );
    runtime.shutdown();
    drop(runtime);

    let (sender, receiver) = mpsc::channel();
    let reopened = NmpRuntime::start(config(path.to_string_lossy().into()), sender).unwrap();
    let attached = reopened
        .reattach_by_correlation(draft().publication_id, &draft().correlation_token)
        .unwrap();
    let PublicationReattachment::Attached(pending) = attached else {
        panic!("retained receipt must attach");
    };
    assert_eq!(pending.receipt_id, receipt_id);
    reopened.attach(pending);
    let replayed = [
        receiver.recv_timeout(Duration::from_secs(2)).unwrap(),
        receiver.recv_timeout(Duration::from_secs(2)).unwrap(),
    ];
    assert_eq!(replayed[0].observation.kind, PublicationFactKind::Accepted);
    assert_eq!(
        replayed[1].observation.kind,
        PublicationFactKind::AwaitingCapability
    );
    reopened.shutdown();
}

#[test]
fn relay_urls_are_reduced_to_stable_opaque_route_ids() {
    let first = observations(WriteStatus::Rejected(
        nmp::RelayUrl::parse("wss://relay.example").unwrap(),
        "x".repeat(800),
    ));
    assert_eq!(first[0].kind, PublicationFactKind::Rejected);
    assert!(first[0].route_id.is_some());
    assert_eq!(first[0].detail.as_ref().unwrap().len(), 512);
    assert!(!format!("{:?}", first[0].route_id).contains("relay.example"));
}

#[test]
fn every_pinned_nmp_write_status_maps_to_exact_product_evidence() {
    let relay = nmp::RelayUrl::parse("wss://relay.example").unwrap();
    let author = nmp::PublicKey::from_hex(&draft().expected_author_hex).unwrap();
    let event = nmp::EventId::all_zeros();
    let statuses = vec![
        WriteStatus::Accepted,
        WriteStatus::Cancelled,
        WriteStatus::AwaitingCapability { pubkey: author },
        WriteStatus::Signed(event),
        WriteStatus::Routed(BTreeSet::from([relay.clone()])),
        WriteStatus::AwaitingRelay {
            relay: relay.clone(),
        },
        WriteStatus::AwaitingAuth {
            relay: relay.clone(),
        },
        WriteStatus::RetryEligible {
            relay: relay.clone(),
            attempt: 1,
            eligible_at: nmp::Timestamp::from(1_u64),
        },
        WriteStatus::HandoffAmbiguous {
            relay: relay.clone(),
            attempt: 2,
            observed_at: nmp::Timestamp::from(2_u64),
        },
        WriteStatus::Sent {
            relay: relay.clone(),
            attempt: 3,
            written_at: nmp::Timestamp::from(3_u64),
        },
        WriteStatus::Acked(relay.clone()),
        WriteStatus::Rejected(relay.clone(), "no".into()),
        WriteStatus::GaveUp(relay.clone()),
        WriteStatus::PersistenceBlocked(relay.clone()),
        WriteStatus::RoutePersistenceBlocked(relay.clone()),
        WriteStatus::OutcomeUnknown(relay),
        WriteStatus::ReplaceableConflict {
            expected: Some(event),
            actual: None,
        },
        WriteStatus::Failed("signer rejected".into()),
    ];
    let kinds = statuses
        .into_iter()
        .flat_map(observations)
        .map(|observation| observation.kind)
        .collect::<Vec<_>>();
    assert_eq!(
        kinds,
        vec![
            PublicationFactKind::Accepted,
            PublicationFactKind::Cancelled,
            PublicationFactKind::AwaitingCapability,
            PublicationFactKind::Signed,
            PublicationFactKind::Routed,
            PublicationFactKind::AwaitingRelay,
            PublicationFactKind::AwaitingAuth,
            PublicationFactKind::RetryEligible,
            PublicationFactKind::HandoffAmbiguous,
            PublicationFactKind::Sent,
            PublicationFactKind::Acknowledged,
            PublicationFactKind::Rejected,
            PublicationFactKind::GaveUp,
            PublicationFactKind::PersistenceBlocked,
            PublicationFactKind::RoutePersistenceBlocked,
            PublicationFactKind::OutcomeUnknown,
            PublicationFactKind::ReplaceableConflict,
            PublicationFactKind::Failed,
        ]
    );
}

#[test]
fn unseen_correlation_is_not_found_without_publishing() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("empty-nmp.redb");
    let (sender, _receiver) = mpsc::channel();
    let runtime = NmpRuntime::start(config(path.to_string_lossy().into()), sender).unwrap();
    let result = runtime
        .reattach_by_correlation(draft().publication_id, &draft().correlation_token)
        .unwrap();
    assert!(matches!(result, PublicationReattachment::NotFound));
    runtime.shutdown();
}

fn scripted_outcome(acknowledge: bool) -> super::PublicationStatusObservation {
    let directory = tempfile::tempdir().unwrap();
    let relay = BoundRelay::bind(acknowledge);
    let secret = "0000000000000000000000000000000000000000000000000000000000000001";
    let event = relay_list_event(secret, &[relay.url()]);
    let relay = relay.start(Some(event));
    let (sender, receiver) = mpsc::channel();
    let runtime = NmpRuntime::start(
        NmpRuntimeConfig {
            store_path: directory
                .path()
                .join("scripted.redb")
                .to_string_lossy()
                .into(),
            indexer_relays: vec![relay.url()],
            app_relays: Vec::new(),
            fallback_relays: Vec::new(),
            allowed_local_relay_hosts: vec!["127.0.0.1".into()],
        },
        sender,
    )
    .unwrap();
    let author = runtime.install_test_account(secret).unwrap();
    assert!(runtime.wait_for_test_relay_list(&author, Duration::from_secs(5)));
    let pending = runtime.publish_tracked(&draft()).unwrap();
    runtime.attach(pending);

    let mut observed = Vec::new();
    let target = if acknowledge {
        PublicationFactKind::Acknowledged
    } else {
        PublicationFactKind::Rejected
    };
    let mut outcome = None;
    for _ in 0..24 {
        let Ok(event) = receiver.recv_timeout(Duration::from_secs(15)) else {
            break;
        };
        observed.push((event.observation.kind, event.observation.route_id));
        if event.observation.kind == target {
            outcome = Some(event.observation);
            break;
        }
    }
    let outcome =
        outcome.unwrap_or_else(|| panic!("scripted relay must emit {target:?}; {observed:?}"));
    assert_eq!(
        relay
            .published()
            .recv_timeout(Duration::from_secs(2))
            .unwrap(),
        30_075
    );
    runtime.shutdown();
    relay.join();
    outcome
}

#[test]
fn scripted_relays_emit_acknowledgement_and_rejection_as_distinct_evidence() {
    let acknowledged = scripted_outcome(true);
    let rejected = scripted_outcome(false);
    assert_eq!(acknowledged.kind, PublicationFactKind::Acknowledged);
    assert_eq!(rejected.kind, PublicationFactKind::Rejected);
    assert!(acknowledged.route_id.is_some());
    assert!(rejected.route_id.is_some());
    assert_eq!(rejected.detail.as_deref(), Some("scripted rejection"));
}
