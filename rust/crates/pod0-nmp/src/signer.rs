use std::collections::BTreeMap;
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};

use nmp::{
    Event, PendingSignerSender, PublicKey, SignerError, SignerOp, SigningCapability, UnsignedEvent,
};
use nostr::prelude::Signature;
use pod0_application::{
    NostrSignatureObservation, NostrSigningRequest, signing_request_is_bounded,
};
use pod0_domain::{HostRequestId, SignerAccountId};
use sha2::{Digest, Sha256};

use crate::NmpAdapterEvent;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeSigningRequest {
    pub request_id: HostRequestId,
    pub request: NostrSigningRequest,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NativeSignerFailure {
    Rejected(String),
    InvalidResponse(String),
    Unavailable,
    TimedOut,
    Disconnected,
}

struct PendingSignature {
    sender: PendingSignerSender<Event>,
    unsigned: UnsignedEvent,
}

pub(crate) struct NativeSignerBridge {
    account_id: SignerAccountId,
    public_key: PublicKey,
    event_sender: mpsc::Sender<NmpAdapterEvent>,
    pending: Mutex<BTreeMap<HostRequestId, PendingSignature>>,
    next_request: AtomicU64,
    available: AtomicBool,
}

impl NativeSignerBridge {
    pub(crate) fn new(
        account_id: SignerAccountId,
        public_key: PublicKey,
        event_sender: mpsc::Sender<NmpAdapterEvent>,
    ) -> Arc<Self> {
        Arc::new(Self {
            account_id,
            public_key,
            event_sender,
            pending: Mutex::new(BTreeMap::new()),
            next_request: AtomicU64::new(1),
            available: AtomicBool::new(true),
        })
    }

    pub(crate) fn capability(self: &Arc<Self>) -> NativeSignerCapability {
        NativeSignerCapability(Arc::clone(self))
    }

    pub(crate) fn complete(
        &self,
        request_id: HostRequestId,
        observation: NostrSignatureObservation,
    ) -> bool {
        let Some(pending) = self.take_pending(request_id) else {
            return false;
        };
        let result = self.validate_result(&pending.unsigned, observation);
        let _ = pending.sender.resolve(result);
        true
    }

    pub(crate) fn fail(&self, request_id: HostRequestId, failure: NativeSignerFailure) -> bool {
        let Some(pending) = self.take_pending(request_id) else {
            return false;
        };
        let _ = pending.sender.resolve(Err(match failure {
            NativeSignerFailure::Rejected(detail) => SignerError::Rejected(detail),
            NativeSignerFailure::InvalidResponse(detail) => SignerError::InvalidResponse(detail),
            NativeSignerFailure::Unavailable => SignerError::Unavailable,
            NativeSignerFailure::TimedOut => SignerError::Timeout,
            NativeSignerFailure::Disconnected => SignerError::Disconnected,
        }));
        true
    }

    pub(crate) fn disconnect(&self) {
        self.available.store(false, Ordering::Release);
        let pending = std::mem::take(
            &mut *self
                .pending
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        );
        for (request_id, pending) in pending {
            let _ = pending.sender.resolve(Err(SignerError::Disconnected));
            let _ = self
                .event_sender
                .send(NmpAdapterEvent::SignerCancelled { request_id });
        }
    }

    fn sign(self: &Arc<Self>, mut unsigned: UnsignedEvent) -> SignerOp<Event> {
        if !self.available.load(Ordering::Acquire) {
            return SignerOp::err(SignerError::Unavailable);
        }
        let event_id_hex = unsigned.id().to_hex();
        let request_id = self.request_id(&event_id_hex);
        let request = NostrSigningRequest {
            account_id: self.account_id,
            event_id_hex,
            expected_author_hex: unsigned.pubkey.to_hex(),
            created_at_seconds: unsigned.created_at.as_secs(),
            kind: unsigned.kind.as_u16(),
            tags: unsigned
                .tags
                .iter()
                .map(|tag| tag.as_slice().to_vec())
                .collect(),
            content: unsigned.content.clone(),
        };
        if !signing_request_is_bounded(&request) {
            return SignerOp::err(SignerError::InvalidResponse(
                "signing request exceeds Pod0 capability bounds".into(),
            ));
        }
        let weak = Arc::downgrade(self);
        let (sender, operation) = SignerOp::pending_channel_with_cancel(move || {
            if let Some(bridge) = weak.upgrade() {
                bridge.cancel(request_id);
            }
        });
        {
            let mut pending = self
                .pending
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if pending.len() >= pod0_application::MAX_PENDING_SIGNER_REQUESTS {
                return SignerOp::err(SignerError::Unavailable);
            }
            pending.insert(request_id, PendingSignature { sender, unsigned });
        }
        if self
            .event_sender
            .send(NmpAdapterEvent::SignerRequest(NativeSigningRequest {
                request_id,
                request,
            }))
            .is_err()
        {
            let _ = self.fail(request_id, NativeSignerFailure::Disconnected);
        }
        operation
    }

    fn validate_result(
        &self,
        unsigned: &UnsignedEvent,
        observation: NostrSignatureObservation,
    ) -> Result<Event, SignerError> {
        if observation.account_id != self.account_id {
            return Err(SignerError::InvalidResponse(
                "native signer account does not match the frozen request".into(),
            ));
        }
        let expected_id = unsigned
            .id
            .expect("the adapter freezes the event id before requesting a signature");
        if observation.event_id_hex != expected_id.to_hex() {
            return Err(SignerError::InvalidResponse(
                "native signer event id does not match the frozen request".into(),
            ));
        }
        let signature = Signature::from_str(&observation.signature_hex).map_err(|_| {
            SignerError::InvalidResponse("native signer returned a malformed signature".into())
        })?;
        unsigned.clone().add_signature(signature).map_err(|_| {
            SignerError::InvalidResponse(
                "native signer signature failed independent NMP verification".into(),
            )
        })
    }

    fn cancel(&self, request_id: HostRequestId) {
        if self.take_pending(request_id).is_some() {
            let _ = self
                .event_sender
                .send(NmpAdapterEvent::SignerCancelled { request_id });
        }
    }

    fn take_pending(&self, request_id: HostRequestId) -> Option<PendingSignature> {
        self.pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&request_id)
    }

    fn request_id(&self, event_id_hex: &str) -> HostRequestId {
        let sequence = self.next_request.fetch_add(1, Ordering::Relaxed);
        let mut digest = Sha256::new();
        digest.update(self.account_id.into_bytes());
        digest.update(event_id_hex.as_bytes());
        digest.update(sequence.to_be_bytes());
        let bytes: [u8; 32] = digest.finalize().into();
        HostRequestId::from_bytes(
            bytes[..16]
                .try_into()
                .expect("SHA-256 always contains sixteen prefix bytes"),
        )
    }
}

pub(crate) struct NativeSignerCapability(Arc<NativeSignerBridge>);

impl SigningCapability for NativeSignerCapability {
    fn public_key(&self) -> Option<PublicKey> {
        Some(self.0.public_key)
    }

    fn is_available(&self) -> bool {
        self.0.available.load(Ordering::Acquire)
    }

    fn sign(&self, unsigned: UnsignedEvent) -> SignerOp<Event> {
        self.0.sign(unsigned)
    }
}

#[cfg(test)]
mod tests;
