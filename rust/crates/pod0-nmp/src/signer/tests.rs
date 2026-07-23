use std::sync::{Arc, mpsc};
use std::time::Duration;

use nmp::{SignerError, SigningCapability, UnsignedEvent};
use nostr::{Keys, Tag};
use pod0_application::NostrSignatureObservation;
use pod0_domain::SignerAccountId;

use super::NativeSignerBridge;
use crate::NmpAdapterEvent;

fn fixture() -> (
    Arc<NativeSignerBridge>,
    mpsc::Receiver<NmpAdapterEvent>,
    Keys,
) {
    let keys = Keys::generate();
    let (sender, receiver) = mpsc::channel();
    (
        NativeSignerBridge::new(
            SignerAccountId::from_bytes([7; 16]),
            keys.public_key(),
            sender,
        ),
        receiver,
        keys,
    )
}

#[test]
fn exact_native_signature_completes_the_frozen_event() {
    let (bridge, events, keys) = fixture();
    let unsigned = UnsignedEvent::new(
        keys.public_key(),
        nmp::Timestamp::from(7),
        nmp::Kind::TextNote,
        Vec::<Tag>::new(),
        "exact",
    );
    let operation = bridge.capability().sign(unsigned);
    let NmpAdapterEvent::SignerRequest(request) =
        events.recv_timeout(Duration::from_secs(1)).unwrap()
    else {
        panic!("signer request expected");
    };
    let signed = UnsignedEvent::new(
        keys.public_key(),
        nmp::Timestamp::from(7),
        nmp::Kind::TextNote,
        Vec::<Tag>::new(),
        "exact",
    )
    .sign_with_keys(&keys)
    .unwrap();
    assert!(bridge.complete(
        request.request_id,
        NostrSignatureObservation {
            account_id: request.request.account_id,
            event_id_hex: signed.id.to_hex(),
            signature_hex: signed.sig.to_string(),
        }
    ));
    assert_eq!(
        operation.wait(Duration::from_secs(1)).unwrap().id,
        signed.id
    );
}

#[test]
fn mismatched_native_signature_fails_closed() {
    let (bridge, events, keys) = fixture();
    let operation = bridge.capability().sign(UnsignedEvent::new(
        keys.public_key(),
        nmp::Timestamp::from(9),
        nmp::Kind::TextNote,
        Vec::<Tag>::new(),
        "frozen",
    ));
    let NmpAdapterEvent::SignerRequest(request) =
        events.recv_timeout(Duration::from_secs(1)).unwrap()
    else {
        panic!("signer request expected");
    };
    let wrong = UnsignedEvent::new(
        keys.public_key(),
        nmp::Timestamp::from(9),
        nmp::Kind::TextNote,
        Vec::<Tag>::new(),
        "different",
    )
    .sign_with_keys(&keys)
    .unwrap();
    assert!(bridge.complete(
        request.request_id,
        NostrSignatureObservation {
            account_id: request.request.account_id,
            event_id_hex: request.request.event_id_hex,
            signature_hex: wrong.sig.to_string(),
        }
    ));
    assert!(matches!(
        operation.wait(Duration::from_secs(1)),
        Err(SignerError::InvalidResponse(_))
    ));
}
