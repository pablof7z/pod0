use pod0_domain::{PublicationId, PublicationRecord, UnixTimestampMilliseconds};

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, uniffi::Record)]
pub struct Pod0PublicationDraft {
    pub publication_id: PublicationId,
    pub expected_author_hex: String,
    pub correlation_token: String,
    pub created_at_seconds: u64,
    pub kind: u16,
    pub tags: Vec<Vec<String>>,
    pub content: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Record)]
pub struct NMPPublicationReceiptLink {
    pub publication_id: PublicationId,
    pub receipt_id: u64,
    pub lease: crate::PersistedEffectLeaseIdentity,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct LeasedNMPPublicationDraft {
    pub lease: crate::PersistedEffectLeaseIdentity,
    pub draft: Pod0PublicationDraft,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct LeasedNMPPublicationReceipt {
    pub lease: crate::PersistedEffectLeaseIdentity,
    pub publication_id: PublicationId,
    pub receipt_id: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct LeasedNMPPublicationObservation {
    pub lease: crate::PersistedEffectLeaseIdentity,
    pub publication_id: PublicationId,
    pub observation: PublicationStatusObservation,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, uniffi::Record)]
pub struct PublicationStatusObservation {
    pub kind: pod0_domain::PublicationFactKind,
    pub route_id: Option<pod0_domain::PublicationRouteId>,
    pub attempt: Option<u64>,
    pub event_id_hex: Option<String>,
    pub observed_at: Option<UnixTimestampMilliseconds>,
    pub detail: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct PublicationsProjection {
    pub items: Vec<PublicationRecord>,
    pub operations: Vec<crate::OperationProjection>,
    pub has_more: bool,
}

impl PublicationsProjection {
    pub fn enforce_bounds(&mut self, offset: usize, maximum: usize) {
        self.items.sort_by_key(|item| {
            (
                std::cmp::Reverse(item.updated_at.value),
                std::cmp::Reverse(item.publication_id),
            )
        });
        self.has_more = self.items.len() > offset.saturating_add(maximum);
        self.items = self
            .items
            .iter()
            .skip(offset)
            .take(maximum)
            .cloned()
            .collect();
        if self.operations.len() > crate::MAX_OPERATION_ITEMS {
            self.operations = self
                .operations
                .iter()
                .rev()
                .take(crate::MAX_OPERATION_ITEMS)
                .cloned()
                .collect::<Vec<_>>();
            self.operations.reverse();
        }
        for item in &mut self.items {
            if item.facts.len() > super::MAX_PUBLICATION_FACTS {
                item.facts = item
                    .facts
                    .iter()
                    .rev()
                    .take(super::MAX_PUBLICATION_FACTS)
                    .cloned()
                    .collect::<Vec<_>>();
                item.facts.reverse();
            }
        }
    }
}
