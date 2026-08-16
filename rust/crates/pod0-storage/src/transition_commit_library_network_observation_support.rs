use pod0_domain::StateRevision;
use sha2::{Digest, Sha256};

use crate::{
    LibraryNetworkObservationAction, LibraryNetworkObservationInput, LibraryNetworkWorkflowRecord,
    StorageError, library_network_store::serialize,
    transition_commit_library_network_admission::request_id,
};

pub(crate) fn shared_episode_id(
    transaction: &rusqlite::Transaction<'_>,
    action: &LibraryNetworkObservationAction,
) -> Result<Option<pod0_domain::EpisodeId>, StorageError> {
    let LibraryNetworkObservationAction::CompleteShared { episode } = action else {
        return Ok(None);
    };
    let feed = episode
        .feed_url
        .as_deref()
        .and_then(pod0_application::normalize_feed_url);
    let parent = crate::library_store_external::resolve_external_parent(
        transaction,
        episode.podcast_id,
        feed.as_ref(),
    )?;
    let guid = episode
        .guid
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(&episode.audio_url);
    Ok(Some(crate::library_store_feed::episode_id(parent, guid)))
}

pub(crate) fn next_effect(
    current: &LibraryNetworkWorkflowRecord,
    action: &LibraryNetworkObservationAction,
    revision: StateRevision,
    now_ms: i64,
) -> Result<Option<pod0_application::DurableLibraryNetworkEffectRequest>, StorageError> {
    let (step, request) = match action {
        LibraryNetworkObservationAction::ContinueTopLookup {
            ranked_ids,
            request,
        } => (
            pod0_application::LibraryNetworkStep::DirectoryLookup {
                ranked_ids: ranked_ids.clone(),
            },
            request,
        ),
        LibraryNetworkObservationAction::ContinueShared { step, request }
        | LibraryNetworkObservationAction::ContinueCatalog { step, request } => {
            (step.clone(), request)
        }
        _ => return Ok(None),
    };
    Ok(Some(pod0_application::DurableLibraryNetworkEffectRequest {
        request_id: request_id(current.command_id, &step),
        command_id: current.command_id,
        cancellation_id: current.cancellation_id,
        issued_revision: revision,
        deadline_at: Some(pod0_domain::UnixTimestampMilliseconds::new(
            now_ms.saturating_add(30_000),
        )),
        step,
        http: request.clone(),
    }))
}

pub(crate) fn observation_identity(
    attempt: pod0_domain::EffectAttemptId,
    sequence: u64,
) -> pod0_domain::EffectAttemptId {
    let mut hash = Sha256::new();
    hash.update(b"pod0/library-network-observation/v1\0");
    hash.update(attempt.into_bytes());
    hash.update(sequence.to_be_bytes());
    let value: [u8; 32] = hash.finalize().into();
    pod0_domain::EffectAttemptId::from_bytes(value[..16].try_into().expect("fixed digest"))
}

pub(crate) fn observation_fingerprint(
    input: &LibraryNetworkObservationInput,
) -> Result<pod0_domain::ContentDigest, StorageError> {
    let mut hash = Sha256::new();
    hash.update(b"pod0/library-network-observation-fingerprint/v1\0");
    hash.update(serialize(&input.observation)?.as_bytes());
    hash.update(serialize(&input.action)?.as_bytes());
    Ok(pod0_domain::ContentDigest::from_bytes(
        hash.finalize().into(),
    ))
}

pub(crate) fn current_revision(
    transaction: &rusqlite::Transaction<'_>,
) -> Result<StateRevision, StorageError> {
    let value: i64 = transaction
        .query_row(
            "SELECT state_revision FROM pod0_playback_state WHERE singleton=1",
            [],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("read library revision", error))?;
    Ok(StateRevision::new(
        u64::try_from(value).map_err(|_| StorageError::InvalidActivity)?,
    ))
}

pub(crate) fn next_revision(value: StateRevision) -> Result<StateRevision, StorageError> {
    value
        .value
        .checked_add(1)
        .map(StateRevision::new)
        .ok_or(StorageError::InvalidActivity)
}

pub(crate) fn require_revision(
    transaction: &rusqlite::Transaction<'_>,
    expected: StateRevision,
) -> Result<(), StorageError> {
    (current_revision(transaction)? == expected)
        .then_some(())
        .ok_or(StorageError::RevisionConflict)
}

pub(crate) fn set_revision(
    transaction: &rusqlite::Transaction<'_>,
    revision: StateRevision,
) -> Result<(), StorageError> {
    transaction
        .execute(
            "UPDATE pod0_playback_state SET state_revision=?1 WHERE singleton=1",
            [i64::try_from(revision.value).map_err(|_| StorageError::InvalidActivity)?],
        )
        .map_err(|error| StorageError::sqlite("advance library revision", error))?;
    Ok(())
}
