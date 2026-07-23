#![forbid(unsafe_code)]

use std::sync::{Arc, Mutex, mpsc};
use std::thread::JoinHandle;

#[cfg(test)]
use nmp::WriteStatus;
use nmp::{Engine, EngineConfig, PublicKey, SignerRegistration};
use pod0_application::NostrSignatureObservation;
#[cfg(test)]
use pod0_application::PublicationStatusObservation;
use pod0_domain::{HostRequestId, SignerAccountId};

mod publication;
mod signer;
mod status;
pub use publication::{
    PendingPublicationReceipt, PublicationAdapterEvent, PublicationReattachment,
};
pub use signer::{NativeSignerFailure, NativeSigningRequest};
#[cfg(test)]
use status::observations;

#[derive(Clone, Debug)]
pub struct NmpRuntimeConfig {
    pub store_path: String,
    pub indexer_relays: Vec<String>,
    pub app_relays: Vec<String>,
    pub fallback_relays: Vec<String>,
    pub allowed_local_relay_hosts: Vec<String>,
}

impl NmpRuntimeConfig {
    #[must_use]
    pub fn production(store_path: String) -> Self {
        Self {
            store_path,
            indexer_relays: vec!["wss://purplepag.es".into()],
            app_relays: vec![
                "wss://relay.primal.net".into(),
                "wss://relay.damus.io".into(),
            ],
            fallback_relays: vec![
                "wss://relay.primal.net".into(),
                "wss://relay.damus.io".into(),
            ],
            allowed_local_relay_hosts: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NmpAdapterEvent {
    Publication(PublicationAdapterEvent),
    SignerRequest(NativeSigningRequest),
    SignerCancelled { request_id: HostRequestId },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NmpAdapterError {
    Engine,
    InvalidAuthor,
    InvalidCorrelation,
    InvalidSignerPublicKey,
    InvalidTag,
    ObserverUnavailable,
}

struct InstalledNativeSigner {
    account_id: SignerAccountId,
    registration: SignerRegistration,
    bridge: Arc<signer::NativeSignerBridge>,
}

pub struct NmpRuntime {
    engine: Arc<Engine>,
    event_sender: mpsc::Sender<NmpAdapterEvent>,
    observers: Mutex<Vec<JoinHandle<()>>>,
    signer: Mutex<Option<InstalledNativeSigner>>,
}

impl NmpRuntime {
    pub fn start(
        config: NmpRuntimeConfig,
        event_sender: mpsc::Sender<NmpAdapterEvent>,
    ) -> Result<Self, NmpAdapterError> {
        let engine = Engine::new(EngineConfig {
            store_path: Some(config.store_path),
            indexer_relays: config.indexer_relays,
            app_relays: config.app_relays,
            fallback_relays: config.fallback_relays,
            allowed_local_relay_hosts: config.allowed_local_relay_hosts,
            ..EngineConfig::default()
        })
        .map_err(|_| NmpAdapterError::Engine)?;
        Ok(Self {
            engine: Arc::new(engine),
            event_sender,
            observers: Mutex::new(Vec::new()),
            signer: Mutex::new(None),
        })
    }

    pub fn install_native_signer(
        &self,
        account_id: SignerAccountId,
        public_key_hex: &str,
    ) -> Result<(), NmpAdapterError> {
        let mut slot = self
            .signer
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let public_key = PublicKey::from_hex(public_key_hex)
            .map_err(|_| NmpAdapterError::InvalidSignerPublicKey)?;
        let bridge =
            signer::NativeSignerBridge::new(account_id, public_key, self.event_sender.clone());
        let registration = self
            .engine
            .add_signer(bridge.capability())
            .map_err(|_| NmpAdapterError::Engine)?;
        if self.engine.set_active_account(Some(public_key)).is_err() {
            let _ = self.engine.remove_signer(registration);
            bridge.disconnect();
            return Err(NmpAdapterError::Engine);
        }
        let replacement = InstalledNativeSigner {
            account_id,
            registration,
            bridge,
        };
        let old = slot.replace(replacement);
        if let Some(old) = old {
            let _ = self.engine.remove_signer(old.registration);
            old.bridge.disconnect();
        }
        Ok(())
    }

    pub fn complete_native_signature(
        &self,
        request_id: HostRequestId,
        observation: NostrSignatureObservation,
    ) -> bool {
        self.signer
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_ref()
            .is_some_and(|signer| signer.bridge.complete(request_id, observation))
    }

    pub fn fail_native_signature(
        &self,
        request_id: HostRequestId,
        failure: NativeSignerFailure,
    ) -> bool {
        self.signer
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_ref()
            .is_some_and(|signer| signer.bridge.fail(request_id, failure))
    }

    pub fn remove_native_signer(
        &self,
        account_id: SignerAccountId,
    ) -> Result<bool, NmpAdapterError> {
        let mut slot = self
            .signer
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if slot.as_ref().map(|signer| signer.account_id) != Some(account_id) {
            return Ok(false);
        }
        self.engine
            .set_active_account(None)
            .map_err(|_| NmpAdapterError::Engine)?;
        let signer = slot.take().expect("matching signer remains installed");
        let removed = self
            .engine
            .remove_signer(signer.registration)
            .map_err(|_| NmpAdapterError::Engine)?;
        signer.bridge.disconnect();
        Ok(removed)
    }

    pub fn shutdown(&self) {
        if let Some(signer) = self
            .signer
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
        {
            let _ = self.engine.set_active_account(None);
            let _ = self.engine.remove_signer(signer.registration);
            signer.bridge.disconnect();
        }
        self.engine.shutdown();
        let handles = std::mem::take(
            &mut *self
                .observers
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        );
        for handle in handles {
            let _ = handle.join();
        }
    }

    #[cfg(test)]
    fn install_test_account(&self, secret_key: &str) -> Result<String, NmpAdapterError> {
        let registration = self
            .engine
            .add_account(secret_key)
            .map_err(|_| NmpAdapterError::Engine)?;
        let public_key =
            PublicKey::from_hex("79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798")
                .map_err(|_| NmpAdapterError::InvalidAuthor)?;
        self.engine
            .set_active_account(Some(public_key))
            .map_err(|_| NmpAdapterError::Engine)?;
        drop(registration);
        Ok(public_key.to_hex())
    }

    #[cfg(test)]
    fn wait_for_test_relay_list(&self, author_hex: &str, timeout: std::time::Duration) -> bool {
        use std::collections::BTreeSet;
        let query = nmp::LiveQuery::from_filter(nmp::Filter {
            kinds: Some(BTreeSet::from([nmp::Kind::RelayList.as_u16()])),
            authors: Some(nmp::Binding::Literal(BTreeSet::from([
                author_hex.to_owned()
            ]))),
            ..nmp::Filter::default()
        });
        let Ok(subscription) = self.engine.observe(query, None) else {
            return false;
        };
        for _ in 0..4 {
            let Ok(frame) = subscription.recv_timeout(timeout) else {
                return false;
            };
            if frame.deltas.iter().any(|delta| {
                matches!(
                    delta,
                    nmp::RowDelta::Added(row) if row.event.kind == nmp::Kind::RelayList
                )
            }) {
                return true;
            }
        }
        false
    }
}

impl Drop for NmpRuntime {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(test)]
mod tests;
