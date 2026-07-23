use pod0_domain::{PublicationArtifactKind, PublicationFactKind, PublicationStage};

use crate::StorageError;

pub(crate) fn artifact_kind_code(kind: PublicationArtifactKind) -> (i64, Option<i64>) {
    match kind {
        PublicationArtifactKind::GeneratedPodcastEpisode => (1, None),
        PublicationArtifactKind::Unsupported { wire_code } => (255, Some(i64::from(wire_code))),
    }
}

pub(crate) fn decode_artifact_kind(
    code: i64,
    wire: Option<i64>,
) -> Result<PublicationArtifactKind, StorageError> {
    match (code, wire) {
        (1, None) => Ok(PublicationArtifactKind::GeneratedPodcastEpisode),
        (255, Some(wire)) => Ok(PublicationArtifactKind::Unsupported {
            wire_code: u32::try_from(wire).map_err(|_| corrupt())?,
        }),
        _ => Err(corrupt()),
    }
}

pub(crate) const fn stage_code(stage: PublicationStage) -> &'static str {
    match stage {
        PublicationStage::Prepared => "prepared",
        PublicationStage::AwaitingCapability => "awaiting_capability",
        PublicationStage::Signing => "signing",
        PublicationStage::Routed => "routed",
        PublicationStage::Delivering => "delivering",
        PublicationStage::EvidenceMixed => "evidence_mixed",
        PublicationStage::Acknowledged => "acknowledged",
        PublicationStage::Rejected => "rejected",
        PublicationStage::Blocked => "blocked",
        PublicationStage::OutcomeUnknown => "outcome_unknown",
        PublicationStage::Cancelled => "cancelled",
        PublicationStage::Failed => "failed",
    }
}

pub(crate) fn decode_stage(value: &str) -> Result<PublicationStage, StorageError> {
    match value {
        "prepared" => Ok(PublicationStage::Prepared),
        "awaiting_capability" => Ok(PublicationStage::AwaitingCapability),
        "signing" => Ok(PublicationStage::Signing),
        "routed" => Ok(PublicationStage::Routed),
        "delivering" => Ok(PublicationStage::Delivering),
        "evidence_mixed" => Ok(PublicationStage::EvidenceMixed),
        "acknowledged" => Ok(PublicationStage::Acknowledged),
        "rejected" => Ok(PublicationStage::Rejected),
        "blocked" => Ok(PublicationStage::Blocked),
        "outcome_unknown" => Ok(PublicationStage::OutcomeUnknown),
        "cancelled" => Ok(PublicationStage::Cancelled),
        "failed" => Ok(PublicationStage::Failed),
        _ => Err(corrupt()),
    }
}

pub(crate) const fn fact_kind_code(kind: PublicationFactKind) -> &'static str {
    match kind {
        PublicationFactKind::Accepted => "accepted",
        PublicationFactKind::Cancelled => "cancelled",
        PublicationFactKind::AwaitingCapability => "awaiting_capability",
        PublicationFactKind::Signed => "signed",
        PublicationFactKind::Routed => "routed",
        PublicationFactKind::AwaitingRelay => "awaiting_relay",
        PublicationFactKind::AwaitingAuth => "awaiting_auth",
        PublicationFactKind::RetryEligible => "retry_eligible",
        PublicationFactKind::HandoffAmbiguous => "handoff_ambiguous",
        PublicationFactKind::Sent => "sent",
        PublicationFactKind::Acknowledged => "acknowledged",
        PublicationFactKind::Rejected => "rejected",
        PublicationFactKind::GaveUp => "gave_up",
        PublicationFactKind::PersistenceBlocked => "persistence_blocked",
        PublicationFactKind::RoutePersistenceBlocked => "route_persistence_blocked",
        PublicationFactKind::OutcomeUnknown => "outcome_unknown",
        PublicationFactKind::ReplaceableConflict => "replaceable_conflict",
        PublicationFactKind::Failed => "failed",
        PublicationFactKind::ReattachmentNotFound => "reattachment_not_found",
        PublicationFactKind::ReattachmentUnreadable => "reattachment_unreadable",
    }
}

pub(crate) fn decode_fact_kind(value: &str) -> Result<PublicationFactKind, StorageError> {
    match value {
        "accepted" => Ok(PublicationFactKind::Accepted),
        "cancelled" => Ok(PublicationFactKind::Cancelled),
        "awaiting_capability" => Ok(PublicationFactKind::AwaitingCapability),
        "signed" => Ok(PublicationFactKind::Signed),
        "routed" => Ok(PublicationFactKind::Routed),
        "awaiting_relay" => Ok(PublicationFactKind::AwaitingRelay),
        "awaiting_auth" => Ok(PublicationFactKind::AwaitingAuth),
        "retry_eligible" => Ok(PublicationFactKind::RetryEligible),
        "handoff_ambiguous" => Ok(PublicationFactKind::HandoffAmbiguous),
        "sent" => Ok(PublicationFactKind::Sent),
        "acknowledged" => Ok(PublicationFactKind::Acknowledged),
        "rejected" => Ok(PublicationFactKind::Rejected),
        "gave_up" => Ok(PublicationFactKind::GaveUp),
        "persistence_blocked" => Ok(PublicationFactKind::PersistenceBlocked),
        "route_persistence_blocked" => Ok(PublicationFactKind::RoutePersistenceBlocked),
        "outcome_unknown" => Ok(PublicationFactKind::OutcomeUnknown),
        "replaceable_conflict" => Ok(PublicationFactKind::ReplaceableConflict),
        "failed" => Ok(PublicationFactKind::Failed),
        "reattachment_not_found" => Ok(PublicationFactKind::ReattachmentNotFound),
        "reattachment_unreadable" => Ok(PublicationFactKind::ReattachmentUnreadable),
        _ => Err(corrupt()),
    }
}

pub(crate) fn stage_from_facts(facts: &[pod0_domain::PublicationFact]) -> PublicationStage {
    use PublicationFactKind as F;
    let has = |target| facts.iter().any(|fact| fact.kind == target);
    let terminal_classes = [
        has(F::Acknowledged),
        has(F::Rejected) || has(F::GaveUp),
        has(F::OutcomeUnknown),
        has(F::Failed) || has(F::ReplaceableConflict),
    ]
    .into_iter()
    .filter(|present| *present)
    .count();
    if terminal_classes > 1 {
        return PublicationStage::EvidenceMixed;
    }
    if has(F::OutcomeUnknown) {
        PublicationStage::OutcomeUnknown
    } else if has(F::Failed) || has(F::ReplaceableConflict) {
        PublicationStage::Failed
    } else if has(F::Cancelled) {
        PublicationStage::Cancelled
    } else if has(F::Acknowledged) {
        PublicationStage::Acknowledged
    } else if has(F::Rejected) || has(F::GaveUp) {
        PublicationStage::Rejected
    } else if has(F::PersistenceBlocked)
        || has(F::RoutePersistenceBlocked)
        || has(F::ReattachmentNotFound)
        || has(F::ReattachmentUnreadable)
    {
        PublicationStage::Blocked
    } else if has(F::Sent)
        || has(F::HandoffAmbiguous)
        || has(F::RetryEligible)
        || has(F::AwaitingRelay)
        || has(F::AwaitingAuth)
    {
        PublicationStage::Delivering
    } else if has(F::Routed) {
        PublicationStage::Routed
    } else if has(F::Signed) {
        PublicationStage::Signing
    } else if has(F::AwaitingCapability) {
        PublicationStage::AwaitingCapability
    } else {
        PublicationStage::Prepared
    }
}

pub(crate) fn fixed<const N: usize>(bytes: Vec<u8>) -> Result<[u8; N], StorageError> {
    bytes.try_into().map_err(|_| corrupt())
}

pub(crate) fn optional_u64(bytes: Option<Vec<u8>>) -> Result<Option<u64>, StorageError> {
    bytes
        .map(|value| fixed::<8>(value).map(u64::from_be_bytes))
        .transpose()
}

fn corrupt() -> StorageError {
    StorageError::CorruptSchema {
        detail: "publication state is malformed",
    }
}
