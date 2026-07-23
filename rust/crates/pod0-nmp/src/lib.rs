#![forbid(unsafe_code)]

use std::sync::{Arc, Mutex, mpsc};
use std::thread::JoinHandle;

use nmp::{
    CorrelationToken, Durability, Engine, EngineConfig, Kind, PublicKey, ReceiptId,
    ReceiptReattachment, Tag, Timestamp, UnsignedEvent, WriteIntent, WritePayload, WriteRouting,
    WriteStatus,
};
use pod0_application::{Pod0PublicationDraft, PublicationStatusObservation};
use pod0_domain::PublicationId;

mod status;
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
pub struct PublicationAdapterEvent {
    pub publication_id: PublicationId,
    pub observation: PublicationStatusObservation,
}

pub struct PendingPublicationReceipt {
    pub publication_id: PublicationId,
    pub receipt_id: u64,
    statuses: mpsc::Receiver<WriteStatus>,
}

pub enum PublicationReattachment {
    Attached(PendingPublicationReceipt),
    NotFound,
    RetainedButUnreadable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NmpAdapterError {
    Engine,
    InvalidAuthor,
    InvalidCorrelation,
    InvalidTag,
    ObserverUnavailable,
}

pub struct NmpRuntime {
    engine: Arc<Engine>,
    event_sender: mpsc::Sender<PublicationAdapterEvent>,
    observers: Mutex<Vec<JoinHandle<()>>>,
}

impl NmpRuntime {
    pub fn start(
        config: NmpRuntimeConfig,
        event_sender: mpsc::Sender<PublicationAdapterEvent>,
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
        })
    }

    pub fn publish_tracked(
        &self,
        draft: &Pod0PublicationDraft,
    ) -> Result<PendingPublicationReceipt, NmpAdapterError> {
        let stream = self
            .engine
            .publish_tracked(write_intent(draft)?)
            .map_err(|_| NmpAdapterError::Engine)?;
        Ok(PendingPublicationReceipt {
            publication_id: draft.publication_id,
            receipt_id: stream.id.0,
            statuses: stream.statuses,
        })
    }

    pub fn reattach_receipt(
        &self,
        publication_id: PublicationId,
        receipt_id: u64,
    ) -> Result<PublicationReattachment, NmpAdapterError> {
        self.convert_reattachment(
            publication_id,
            self.engine
                .reattach_receipt(ReceiptId(receipt_id))
                .map_err(|_| NmpAdapterError::Engine)?,
        )
    }

    pub fn reattach_by_correlation(
        &self,
        publication_id: PublicationId,
        correlation_token: &str,
    ) -> Result<PublicationReattachment, NmpAdapterError> {
        self.convert_reattachment(
            publication_id,
            self.engine
                .reattach_by_correlation(correlation_token.to_owned())
                .map_err(|_| NmpAdapterError::Engine)?,
        )
    }

    pub fn attach(&self, receipt: PendingPublicationReceipt) {
        let sender = self.event_sender.clone();
        let handle = std::thread::spawn(move || {
            while let Ok(status) = receipt.statuses.recv() {
                for observation in observations(status) {
                    if sender
                        .send(PublicationAdapterEvent {
                            publication_id: receipt.publication_id,
                            observation,
                        })
                        .is_err()
                    {
                        return;
                    }
                }
            }
        });
        self.observers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(handle);
    }

    pub fn shutdown(&self) {
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

    fn convert_reattachment(
        &self,
        publication_id: PublicationId,
        value: ReceiptReattachment,
    ) -> Result<PublicationReattachment, NmpAdapterError> {
        Ok(match value {
            ReceiptReattachment::Attached(id, statuses) => {
                PublicationReattachment::Attached(PendingPublicationReceipt {
                    publication_id,
                    receipt_id: id.0,
                    statuses,
                })
            }
            ReceiptReattachment::NotFound => PublicationReattachment::NotFound,
            ReceiptReattachment::RetainedButUnreadable => {
                PublicationReattachment::RetainedButUnreadable
            }
        })
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

fn write_intent(draft: &Pod0PublicationDraft) -> Result<WriteIntent, NmpAdapterError> {
    let author = PublicKey::from_hex(&draft.expected_author_hex)
        .map_err(|_| NmpAdapterError::InvalidAuthor)?;
    let correlation = CorrelationToken::try_from(draft.correlation_token.as_str())
        .map_err(|_| NmpAdapterError::InvalidCorrelation)?;
    let tags = draft
        .tags
        .iter()
        .cloned()
        .map(Tag::parse)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| NmpAdapterError::InvalidTag)?;
    Ok(WriteIntent {
        payload: WritePayload::Unsigned(UnsignedEvent::new(
            author,
            Timestamp::from(draft.created_at_seconds),
            Kind::from(draft.kind),
            tags,
            draft.content.clone(),
        )),
        durability: Durability::Durable,
        routing: WriteRouting::AuthorOutbox,
        identity_override: Some(author),
        correlation: Some(correlation),
    })
}

#[cfg(test)]
mod tests;
