use std::sync::mpsc;

use nmp::{
    CorrelationToken, Durability, Kind, PublicKey, ReceiptId, ReceiptReattachment, Tag, Timestamp,
    UnsignedEvent, WriteIntent, WritePayload, WriteRouting, WriteStatus,
};
use pod0_application::{Pod0PublicationDraft, PublicationStatusObservation};
use pod0_domain::PublicationId;

use crate::status::observations;
use crate::{NmpAdapterError, NmpAdapterEvent, NmpRuntime};

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

impl NmpRuntime {
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
                        .send(NmpAdapterEvent::Publication(PublicationAdapterEvent {
                            publication_id: receipt.publication_id,
                            observation,
                        }))
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
