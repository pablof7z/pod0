use std::sync::mpsc;

use pod0_application::PublicationStatusObservation;
use pod0_domain::{PublicationFactKind, PublicationId};
use pod0_nmp::{
    NmpAdapterEvent, NmpRuntime, NmpRuntimeConfig, PublicationAdapterEvent, PublicationReattachment,
};

use crate::{FacadeOpenError, Pod0Facade};

impl Pod0Facade {
    pub(super) fn start_nmp(&self) -> Result<(), FacadeOpenError> {
        let mut runtime_slot = self
            .nmp
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if runtime_slot.is_some() {
            return Ok(());
        }
        let nmp_path = self
            .nmp_store_path
            .clone()
            .ok_or(FacadeOpenError::StorageUnavailable)?;
        let (sender, receiver) = mpsc::channel();
        let runtime = NmpRuntime::start(runtime_config(nmp_path), sender)
            .map_err(|_| FacadeOpenError::StorageUnavailable)?;
        *runtime_slot = Some(runtime);
        drop(runtime_slot);
        let state = std::sync::Arc::clone(&self.state);
        let dispatcher = std::thread::spawn(move || {
            while let Ok(event) = receiver.recv() {
                let deliveries = {
                    let mut state = state
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    let changed = match event {
                        NmpAdapterEvent::Publication(event) => state.apply_publication_event(event),
                        NmpAdapterEvent::SignerRequest(request) => {
                            state.enqueue_native_signing_request(request)
                        }
                        NmpAdapterEvent::SignerCancelled { request_id } => {
                            state.cancel_native_signing_request(request_id)
                        }
                    };
                    if changed {
                        state.deliveries()
                    } else {
                        Vec::new()
                    }
                };
                for (subscriber, projection) in deliveries {
                    subscriber.receive(projection);
                }
            }
        });
        *self
            .nmp_dispatcher
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(dispatcher);
        Ok(())
    }

    pub(super) fn drive_pending_publications(&self) {
        let drafts = self.state().take_pending_publications();
        if drafts.is_empty() {
            return;
        }
        if self.start_nmp().is_err() {
            for draft in drafts {
                self.record_adapter_failure(draft.publication_id);
            }
            return;
        }
        let runtime = self
            .nmp
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        for draft in drafts {
            let Some(runtime) = runtime.as_ref() else {
                self.record_adapter_failure(draft.publication_id);
                continue;
            };
            match runtime.publish_tracked(&draft) {
                Ok(receipt) => {
                    let persisted = self
                        .state()
                        .record_publication_receipt(draft.publication_id, receipt.receipt_id);
                    if persisted || self.receipt_is_persisted(&draft, receipt.receipt_id) {
                        runtime.attach(receipt);
                    }
                }
                Err(_) => self.record_adapter_failure(draft.publication_id),
            }
        }
    }

    pub(super) fn recover_nmp_publications(&self) {
        let records = self.state().publication_records();
        let runtime = self
            .nmp
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(runtime) = runtime.as_ref() else {
            return;
        };
        for record in records {
            let reattachment = if let Some(receipt_id) = record.receipt_id {
                runtime.reattach_receipt(record.publication_id, receipt_id)
            } else {
                runtime.reattach_by_correlation(record.publication_id, &record.correlation_token)
            };
            match reattachment {
                Ok(PublicationReattachment::Attached(receipt)) => {
                    let _ = self
                        .state()
                        .record_publication_receipt(record.publication_id, receipt.receipt_id);
                    runtime.attach(receipt);
                }
                Ok(PublicationReattachment::NotFound) => self.record_reattachment_fact(
                    record.publication_id,
                    PublicationFactKind::ReattachmentNotFound,
                ),
                Ok(PublicationReattachment::RetainedButUnreadable) => self
                    .record_reattachment_fact(
                        record.publication_id,
                        PublicationFactKind::ReattachmentUnreadable,
                    ),
                Err(_) => self.record_adapter_failure(record.publication_id),
            }
        }
    }

    fn receipt_is_persisted(
        &self,
        draft: &pod0_application::Pod0PublicationDraft,
        receipt_id: u64,
    ) -> bool {
        self.state()
            .publication_store
            .as_ref()
            .and_then(|store| store.publication(draft.publication_id).ok())
            .flatten()
            .is_some_and(|record| record.receipt_id == Some(receipt_id))
    }

    fn record_adapter_failure(&self, publication_id: PublicationId) {
        self.record_observation(
            publication_id,
            PublicationFactKind::Failed,
            Some("NMP adapter rejected the durable publication".into()),
        );
    }

    fn record_reattachment_fact(&self, publication_id: PublicationId, kind: PublicationFactKind) {
        self.record_observation(publication_id, kind, None);
    }

    fn record_observation(
        &self,
        publication_id: PublicationId,
        kind: PublicationFactKind,
        detail: Option<String>,
    ) {
        let changed = self
            .state()
            .apply_publication_event(PublicationAdapterEvent {
                publication_id,
                observation: PublicationStatusObservation {
                    kind,
                    route_id: None,
                    attempt: None,
                    event_id_hex: None,
                    observed_at: None,
                    detail,
                },
            });
        if changed {
            self.notify_subscribers();
        }
    }
}

#[cfg(test)]
fn runtime_config(store_path: String) -> NmpRuntimeConfig {
    NmpRuntimeConfig {
        store_path,
        indexer_relays: Vec::new(),
        app_relays: Vec::new(),
        fallback_relays: Vec::new(),
        allowed_local_relay_hosts: Vec::new(),
    }
}

#[cfg(not(test))]
fn runtime_config(store_path: String) -> NmpRuntimeConfig {
    NmpRuntimeConfig::production(store_path)
}
