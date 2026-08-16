use pod0_application::{
    PublicationPrepareActivityInput, compose_generated_episode_publication,
    initial_publication_record, plan_publication_prepare, validate_publication_intent,
};
use pod0_domain::{
    CommandId, ContentDigest, EpisodeRecord, PodcastRecord, PublicationIntent, StateRevision,
    UnixTimestampMilliseconds,
};

use super::TransitionCommit;
use crate::publication_store_read::read_publication;
use crate::publication_store_write::{
    command_receipt, insert_command, insert_publication, same_semantics,
};
use crate::{PublicationPrepareOutcome, StorageError, TransitionIngress, TransitionIngressKind};

pub(crate) fn commit_publication_prepare(
    path: &std::path::Path,
    command_id: CommandId,
    command_fingerprint: &str,
    intent: &PublicationIntent,
    episode: &EpisodeRecord,
    podcast: &PodcastRecord,
    prepared_at: UnixTimestampMilliseconds,
) -> Result<PublicationPrepareOutcome, StorageError> {
    validate_publication_intent(intent).map_err(|_| StorageError::InvalidPublication)?;
    let candidate = initial_publication_record(intent, episode, prepared_at);
    let draft = compose_generated_episode_publication(&candidate, episode, podcast)
        .map_err(|_| StorageError::InvalidPublication)?;
    let fingerprint = decode_fingerprint(command_fingerprint)?;
    let duplicate = std::cell::Cell::new(false);
    let receipt = TransitionCommit::open(path)?.commit_planned_with(
        TransitionIngress {
            kind: TransitionIngressKind::ApplicationCommand,
            id: command_id.into_bytes(),
            fingerprint,
        },
        prepared_at,
        |transaction| {
            if let Some(existing) = command_receipt(transaction, command_id, command_fingerprint)? {
                duplicate.set(true);
                return plan_publication_prepare(PublicationPrepareActivityInput {
                    command_id,
                    current_revision: existing.revision,
                    committed_revision: existing.revision,
                    disposition: pod0_application::RequestDisposition::Duplicate,
                    draft: None,
                })
                .map_err(|_| StorageError::InvalidActivity);
            }
            if let Some(existing) = read_publication(transaction, candidate.publication_id)? {
                if !same_semantics(&existing, &candidate) {
                    return Err(StorageError::PublicationConflict);
                }
                duplicate.set(true);
                return plan_publication_prepare(PublicationPrepareActivityInput {
                    command_id,
                    current_revision: existing.revision,
                    committed_revision: existing.revision,
                    disposition: pod0_application::RequestDisposition::Duplicate,
                    draft: None,
                })
                .map_err(|_| StorageError::InvalidActivity);
            }
            plan_publication_prepare(PublicationPrepareActivityInput {
                command_id,
                current_revision: StateRevision::INITIAL,
                committed_revision: candidate.revision,
                disposition: pod0_application::RequestDisposition::Accepted,
                draft: Some(draft.clone()),
            })
            .map_err(|_| StorageError::InvalidActivity)
        },
        |transaction, expected, _| {
            if duplicate.get() {
                return Ok(expected);
            }
            insert_publication(transaction, &candidate)?;
            insert_command(
                transaction,
                command_id,
                command_fingerprint,
                candidate.publication_id,
                prepared_at,
            )?;
            Ok(candidate.revision)
        },
    )?;
    let store = crate::PublicationStore::open(path)?;
    let record = store
        .publication(candidate.publication_id)?
        .ok_or(StorageError::PublicationNotFound)?;
    if receipt.replayed || duplicate.get() {
        Ok(PublicationPrepareOutcome::Duplicate(record))
    } else {
        Ok(PublicationPrepareOutcome::Applied(record))
    }
}

fn decode_fingerprint(value: &str) -> Result<ContentDigest, StorageError> {
    if value.len() != 64 {
        return Err(StorageError::InvalidPublication);
    }
    let mut bytes = [0_u8; 32];
    for (index, chunk) in value.as_bytes().chunks_exact(2).enumerate() {
        let text = std::str::from_utf8(chunk).map_err(|_| StorageError::InvalidPublication)?;
        bytes[index] =
            u8::from_str_radix(text, 16).map_err(|_| StorageError::InvalidPublication)?;
    }
    Ok(ContentDigest::from_bytes(bytes))
}
