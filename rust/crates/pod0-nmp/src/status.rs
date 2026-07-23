use nmp::WriteStatus;
use pod0_application::{MAX_PUBLICATION_DETAIL_BYTES, PublicationStatusObservation};
use pod0_domain::{PublicationFactKind, PublicationRouteId, UnixTimestampMilliseconds};
use sha2::{Digest as _, Sha256};

pub(super) fn observations(status: WriteStatus) -> Vec<PublicationStatusObservation> {
    use PublicationFactKind as F;
    match status {
        WriteStatus::Accepted => vec![observation(F::Accepted)],
        WriteStatus::Cancelled => vec![observation(F::Cancelled)],
        WriteStatus::AwaitingCapability { pubkey } => {
            vec![with_detail(F::AwaitingCapability, pubkey.to_hex())]
        }
        WriteStatus::Signed(id) => vec![with_event(F::Signed, id.to_hex())],
        WriteStatus::Routed(relays) => relays
            .into_iter()
            .map(|relay| with_route(F::Routed, &relay.to_string()))
            .collect(),
        WriteStatus::AwaitingRelay { relay } => {
            vec![with_route(F::AwaitingRelay, &relay.to_string())]
        }
        WriteStatus::AwaitingAuth { relay } => {
            vec![with_route(F::AwaitingAuth, &relay.to_string())]
        }
        WriteStatus::RetryEligible {
            relay,
            attempt,
            eligible_at,
        } => vec![with_attempt_time(
            F::RetryEligible,
            &relay.to_string(),
            attempt,
            eligible_at.as_secs(),
        )],
        WriteStatus::HandoffAmbiguous {
            relay,
            attempt,
            observed_at,
        } => vec![with_attempt_time(
            F::HandoffAmbiguous,
            &relay.to_string(),
            attempt,
            observed_at.as_secs(),
        )],
        WriteStatus::Sent {
            relay,
            attempt,
            written_at,
        } => vec![with_attempt_time(
            F::Sent,
            &relay.to_string(),
            attempt,
            written_at.as_secs(),
        )],
        WriteStatus::Acked(relay) => {
            vec![with_route(F::Acknowledged, &relay.to_string())]
        }
        WriteStatus::Rejected(relay, reason) => {
            vec![with_route_detail(F::Rejected, &relay.to_string(), reason)]
        }
        WriteStatus::GaveUp(relay) => vec![with_route(F::GaveUp, &relay.to_string())],
        WriteStatus::PersistenceBlocked(relay) => {
            vec![with_route(F::PersistenceBlocked, &relay.to_string())]
        }
        WriteStatus::RoutePersistenceBlocked(relay) => {
            vec![with_route(F::RoutePersistenceBlocked, &relay.to_string())]
        }
        WriteStatus::OutcomeUnknown(relay) => {
            vec![with_route(F::OutcomeUnknown, &relay.to_string())]
        }
        WriteStatus::ReplaceableConflict { expected, actual } => vec![with_detail(
            F::ReplaceableConflict,
            format!(
                "expected={};actual={}",
                expected.map_or_else(|| "none".into(), |id| id.to_hex()),
                actual.map_or_else(|| "none".into(), |id| id.to_hex())
            ),
        )],
        WriteStatus::Failed(reason) => vec![with_detail(F::Failed, reason)],
    }
}

fn observation(kind: PublicationFactKind) -> PublicationStatusObservation {
    PublicationStatusObservation {
        kind,
        route_id: None,
        attempt: None,
        event_id_hex: None,
        observed_at: None,
        detail: None,
    }
}

fn with_event(kind: PublicationFactKind, event_id_hex: String) -> PublicationStatusObservation {
    PublicationStatusObservation {
        event_id_hex: Some(event_id_hex),
        ..observation(kind)
    }
}

fn with_detail(kind: PublicationFactKind, detail: String) -> PublicationStatusObservation {
    PublicationStatusObservation {
        detail: Some(bounded(detail)),
        ..observation(kind)
    }
}

fn with_route(kind: PublicationFactKind, relay: &str) -> PublicationStatusObservation {
    PublicationStatusObservation {
        route_id: Some(route_id(relay)),
        ..observation(kind)
    }
}

fn with_route_detail(
    kind: PublicationFactKind,
    relay: &str,
    detail: String,
) -> PublicationStatusObservation {
    PublicationStatusObservation {
        detail: Some(bounded(detail)),
        ..with_route(kind, relay)
    }
}

fn with_attempt_time(
    kind: PublicationFactKind,
    relay: &str,
    attempt: u64,
    seconds: u64,
) -> PublicationStatusObservation {
    PublicationStatusObservation {
        attempt: Some(attempt),
        observed_at: Some(UnixTimestampMilliseconds::new(
            i64::try_from(seconds.saturating_mul(1_000)).unwrap_or(i64::MAX),
        )),
        ..with_route(kind, relay)
    }
}

fn route_id(relay: &str) -> PublicationRouteId {
    let digest = Sha256::digest(relay.as_bytes());
    PublicationRouteId::from_bytes(digest[..16].try_into().expect("digest slice"))
}

fn bounded(mut value: String) -> String {
    while value.len() > MAX_PUBLICATION_DETAIL_BYTES {
        value.pop();
    }
    value
}
