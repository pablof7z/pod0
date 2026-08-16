use pod0_application::{
    EffectOutcome, LeasedNMPPublicationObservation, LeasedNMPPublicationReceipt,
    PublicationObservationActivityInput, PublicationStatusObservation,
    plan_publication_observation,
};
use pod0_domain::{ContentDigest, HostRequestId, PublicationFactKind, PublicationId};
use rusqlite::params;
use sha2::{Digest as _, Sha256};

use super::TransitionCommit;
use crate::publication_store_observe::observe_in_transaction;
use crate::publication_store_read::read_publication;
use crate::publication_store_write::record_receipt_in_transaction;
use crate::{StorageError, TransitionIngress, TransitionIngressKind};

pub(crate) fn commit_publication_receipt(
    path: &std::path::Path,
    input: LeasedNMPPublicationReceipt,
    committed_at: pod0_domain::UnixTimestampMilliseconds,
) -> Result<pod0_domain::PublicationRecord, StorageError> {
    commit_publication_progress(
        path,
        input.lease,
        input.publication_id,
        committed_at,
        receipt_fingerprint(&input),
        EffectOutcome::Progressed,
        false,
        |transaction| {
            record_receipt_in_transaction(
                transaction,
                input.publication_id,
                input.receipt_id,
                committed_at,
            )
        },
    )
}

pub(crate) fn commit_publication_observation(
    path: &std::path::Path,
    input: LeasedNMPPublicationObservation,
    committed_at: pod0_domain::UnixTimestampMilliseconds,
) -> Result<pod0_domain::PublicationRecord, StorageError> {
    let outcome = publication_outcome(&input.observation);
    let terminal = publication_terminal(&input.observation);
    commit_publication_progress(
        path,
        input.lease,
        input.publication_id,
        committed_at,
        observation_fingerprint(&input),
        outcome,
        terminal,
        |transaction| {
            observe_in_transaction(
                transaction,
                input.publication_id,
                &input.observation,
                committed_at,
            )
        },
    )
}

#[allow(clippy::too_many_arguments)]
fn commit_publication_progress(
    path: &std::path::Path,
    lease: pod0_application::PersistedEffectLeaseIdentity,
    publication_id: PublicationId,
    committed_at: pod0_domain::UnixTimestampMilliseconds,
    fingerprint: ContentDigest,
    outcome: EffectOutcome,
    terminal: bool,
    mutate_publication: impl FnOnce(
        &rusqlite::Transaction<'_>,
    ) -> Result<pod0_domain::PublicationRecord, StorageError>,
) -> Result<pod0_domain::PublicationRecord, StorageError> {
    let next = std::cell::RefCell::new(None);
    let receipt = TransitionCommit::open(path)?.commit_planned_with_transaction_hooks(
        TransitionIngress {
            kind: TransitionIngressKind::HostObservation,
            id: fingerprint_id(fingerprint),
            fingerprint,
        },
        committed_at,
        |transaction| {
            validate_lease(transaction, lease, publication_id, committed_at)?;
            let current = read_publication(transaction, publication_id)?
                .ok_or(StorageError::PublicationNotFound)?;
            let committed_revision = current
                .revision
                .value
                .checked_add(1)
                .map(pod0_domain::StateRevision::new)
                .ok_or(StorageError::PublicationConflict)?;
            plan_publication_observation(PublicationObservationActivityInput {
                request_id: HostRequestId::from_bytes(fingerprint_id(fingerprint)),
                publication_id,
                current_revision: current.revision,
                committed_revision,
                intent_id: lease.intent_id,
                observation_activity_id: pod0_domain::EffectAttemptId::from_bytes(fingerprint_id(
                    fingerprint,
                )),
                attempt_id: lease.attempt_id,
                authorizing_activity_id: lease.authorizing_activity_id,
                correlation_id: lease.correlation_id,
                outcome,
            })
            .map_err(|_| StorageError::InvalidActivity)
        },
        |transaction| stage_effect(transaction, lease, fingerprint, outcome),
        |transaction, expected, _| {
            let record = mutate_publication(transaction)?;
            if record.revision.value <= expected.value {
                return Err(StorageError::PublicationConflict);
            }
            *next.borrow_mut() = Some(record.clone());
            Ok(record.revision)
        },
        |transaction| {
            if terminal {
                complete_effect(transaction, lease)?;
            } else {
                continue_effect(transaction, lease)?;
            }
            Ok(())
        },
    )?;
    if receipt.replayed {
        return crate::PublicationStore::open(path)?
            .publication(publication_id)?
            .ok_or(StorageError::PublicationNotFound);
    }
    next.into_inner().ok_or(StorageError::InvalidActivity)
}

fn validate_lease(
    transaction: &rusqlite::Transaction<'_>,
    lease: pod0_application::PersistedEffectLeaseIdentity,
    publication_id: PublicationId,
    committed_at: pod0_domain::UnixTimestampMilliseconds,
) -> Result<(), StorageError> {
    let fence = i64::try_from(lease.fence).map_err(|_| StorageError::InvalidActivity)?;
    let valid: bool = transaction
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM pod0_effect_attempts a JOIN pod0_effect_intents i \
             ON i.intent_id=a.intent_id WHERE a.lease_id=?1 AND a.attempt_id=?2 AND a.intent_id=?3 \
             AND a.fence=?4 AND a.state_code=1 AND (a.lease_expires_at_ms>=?5 OR \
             EXISTS(SELECT 1 FROM pod0_publications started WHERE \
             started.publication_id=i.subject_id AND started.receipt_id IS NOT NULL)) \
             AND a.lease_expires_at_ms=?6 AND i.authorizing_activity_id=?7 \
             AND i.correlation_id=?8 AND i.effect_kind_code=14 AND i.subject_code=7 \
             AND i.subject_id=?9)",
            params![
                lease.lease_id.into_bytes().as_slice(),
                lease.attempt_id.into_bytes().as_slice(),
                lease.intent_id.into_bytes().as_slice(),
                fence,
                committed_at.value,
                lease.expires_at.value,
                lease.authorizing_activity_id.into_bytes().as_slice(),
                lease.correlation_id.into_bytes().as_slice(),
                publication_id.into_bytes().as_slice(),
            ],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("validate publication effect lease", error))?;
    valid.then_some(()).ok_or(StorageError::PublicationConflict)
}

fn stage_effect(
    transaction: &rusqlite::Transaction<'_>,
    lease: pod0_application::PersistedEffectLeaseIdentity,
    fingerprint: ContentDigest,
    outcome: EffectOutcome,
) -> Result<(), StorageError> {
    let changed = transaction
        .execute(
            "UPDATE pod0_effect_attempts SET state_code=2,observation_schema_version=1,\
             observation_json=?1,outcome_schema_version=1,outcome_json=?2,observed_at_ms=?3 \
             WHERE lease_id=?4 AND fence=?5 AND state_code=1",
            params![
                hex(&fingerprint.into_bytes()),
                serde_json::to_string(&outcome).map_err(|_| StorageError::InvalidActivity)?,
                lease.expires_at.value,
                lease.lease_id.into_bytes().as_slice(),
                i64::try_from(lease.fence).map_err(|_| StorageError::InvalidActivity)?,
            ],
        )
        .map_err(|error| StorageError::sqlite("stage publication effect observation", error))?;
    (changed == 1)
        .then_some(())
        .ok_or(StorageError::PublicationConflict)
}

fn continue_effect(
    transaction: &rusqlite::Transaction<'_>,
    lease: pod0_application::PersistedEffectLeaseIdentity,
) -> Result<(), StorageError> {
    let changed = transaction
        .execute(
            "UPDATE pod0_effect_attempts SET state_code=1 WHERE lease_id=?1 AND fence=?2 \
             AND state_code=2",
            params![
                lease.lease_id.into_bytes().as_slice(),
                i64::try_from(lease.fence).map_err(|_| StorageError::InvalidActivity)?,
            ],
        )
        .map_err(|error| StorageError::sqlite("continue publication effect", error))?;
    (changed == 1)
        .then_some(())
        .ok_or(StorageError::PublicationConflict)
}

fn complete_effect(
    transaction: &rusqlite::Transaction<'_>,
    lease: pod0_application::PersistedEffectLeaseIdentity,
) -> Result<(), StorageError> {
    crate::effect_outbox::complete_host_observation_in_transaction(transaction, lease)
        .map_err(|_| StorageError::PublicationConflict)
}

fn publication_terminal(observation: &PublicationStatusObservation) -> bool {
    matches!(
        observation.kind,
        PublicationFactKind::Cancelled
            | PublicationFactKind::Rejected
            | PublicationFactKind::GaveUp
            | PublicationFactKind::OutcomeUnknown
            | PublicationFactKind::ReplaceableConflict
            | PublicationFactKind::Failed
            | PublicationFactKind::ReattachmentNotFound
            | PublicationFactKind::ReattachmentUnreadable
    )
}

fn publication_outcome(observation: &PublicationStatusObservation) -> EffectOutcome {
    match observation.kind {
        PublicationFactKind::Cancelled => EffectOutcome::Cancelled,
        PublicationFactKind::Acknowledged => EffectOutcome::Succeeded,
        PublicationFactKind::OutcomeUnknown
        | PublicationFactKind::ReattachmentNotFound
        | PublicationFactKind::ReattachmentUnreadable => EffectOutcome::OutcomeUnknown,
        PublicationFactKind::Rejected
        | PublicationFactKind::GaveUp
        | PublicationFactKind::ReplaceableConflict
        | PublicationFactKind::Failed => EffectOutcome::Failed {
            code: pod0_application::ActivityFailureCode::ProviderUnavailable,
        },
        _ => EffectOutcome::Progressed,
    }
}

fn receipt_fingerprint(input: &LeasedNMPPublicationReceipt) -> ContentDigest {
    hash(&[
        b"receipt",
        &input.lease.attempt_id.into_bytes(),
        &input.receipt_id.to_be_bytes(),
    ])
}

fn observation_fingerprint(input: &LeasedNMPPublicationObservation) -> ContentDigest {
    let encoded = serde_json::to_vec(&input.observation).expect("typed publication observation");
    hash(&[
        b"observation",
        &input.lease.attempt_id.into_bytes(),
        &encoded,
    ])
}

fn hash(parts: &[&[u8]]) -> ContentDigest {
    let mut hash = Sha256::new();
    hash.update(b"pod0/publication/effect-observation/v1");
    for part in parts {
        hash.update((part.len() as u64).to_be_bytes());
        hash.update(part);
    }
    ContentDigest::from_bytes(hash.finalize().into())
}

fn fingerprint_id(fingerprint: ContentDigest) -> [u8; 16] {
    fingerprint.into_bytes()[..16]
        .try_into()
        .expect("fixed digest prefix")
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
