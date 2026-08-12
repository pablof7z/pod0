use pod0_application::{PlaybackActivityInput, PlaybackTransition, plan_playback_activity};
use pod0_domain::{CommandId, EpisodeId, UnixTimestampMilliseconds};

use super::TransitionCommit;
use super::application_support::{fingerprint as fingerprint_digest, legacy_library_receipt};
use crate::library_store::finish_command;
use crate::library_store_playback_apply::apply_mutation;
use crate::{
    PlaybackMutation, PlaybackMutationResult, StorageError, TransitionIngress,
    TransitionIngressKind,
};

#[allow(clippy::too_many_arguments)]
pub(crate) fn commit_playback_mutation(
    path: &std::path::Path,
    command_id: CommandId,
    fingerprint: &str,
    mutation: PlaybackMutation,
    episode_id: Option<EpisodeId>,
    transition: PlaybackTransition,
    internal_command: Option<pod0_application::DurableInternalCommandRequest>,
    observed_at_ms: i64,
) -> Result<PlaybackMutationResult, StorageError> {
    let store = crate::LibraryStore::open_authoritative(path)?;
    let reused = std::cell::Cell::new(false);
    let ingress = TransitionIngress {
        kind: TransitionIngressKind::ApplicationCommand,
        id: command_id.into_bytes(),
        fingerprint: fingerprint_digest(fingerprint)?,
    };
    let receipt = TransitionCommit::open(path)?.commit_planned_with(
        ingress,
        UnixTimestampMilliseconds::new(observed_at_ms),
        |transaction| {
            let current = playback_revision(transaction)?;
            let legacy = legacy_library_receipt(
                transaction,
                command_id,
                fingerprint,
                "read playback command",
            )?;
            plan_playback_activity(PlaybackActivityInput {
                command_id,
                episode_id,
                current_revision: current,
                legacy_command_revision: legacy,
                transition,
                internal_command,
            })
            .map(|plan| plan.map_mutation(|()| legacy))
            .map_err(|_| StorageError::InvalidActivity)
        },
        |transaction, expected, legacy| {
            if playback_revision(transaction)? != expected {
                return Err(StorageError::RevisionConflict);
            }
            if let Some(value) = legacy {
                reused.set(true);
                return Ok(value);
            }
            apply_mutation(transaction, mutation, observed_at_ms)?;
            let value = finish_command(transaction, command_id, fingerprint, observed_at_ms)?;
            Ok(value)
        },
    )?;
    let active = store.snapshot()?.playback.active_episode_id;
    Ok(PlaybackMutationResult {
        revision: receipt.committed_revision,
        active_episode_id: active,
        reused_existing: receipt.replayed || reused.get(),
    })
}

fn playback_revision(
    connection: &rusqlite::Connection,
) -> Result<pod0_domain::StateRevision, StorageError> {
    let value: i64 = connection
        .query_row(
            "SELECT state_revision FROM pod0_playback_state WHERE singleton=1",
            [],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("read playback activity revision", error))?;
    u64::try_from(value)
        .map(pod0_domain::StateRevision::new)
        .map_err(|_| StorageError::InvalidActivity)
}
