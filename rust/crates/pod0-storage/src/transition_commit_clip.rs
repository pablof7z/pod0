use pod0_application::{
    ActivitySubject, RequestDisposition, RequestRejectionReason, UserArtifactActivityInput,
    UserArtifactMutation, UserArtifactTransition, plan_user_artifact_activity,
};
use pod0_domain::{
    ClipId, ClipSource, CommandId, EpisodeId, PodcastId, SpeakerId, StateRevision,
    UnixTimestampMilliseconds,
};

use super::TransitionCommit;
use super::application_support::{fingerprint, legacy_library_receipt, next_core_revision};
use crate::{StorageError, TransitionIngress, TransitionIngressKind};

#[allow(clippy::too_many_arguments)]
pub(crate) fn commit_clip_create(
    path: &std::path::Path,
    command_id: CommandId,
    command_fingerprint: &str,
    clip_id: ClipId,
    episode_id: EpisodeId,
    podcast_id: PodcastId,
    start_milliseconds: u64,
    end_milliseconds: u64,
    caption: Option<&str>,
    speaker_id: Option<SpeakerId>,
    frozen_transcript_text: &str,
    source: ClipSource,
    observed_at_ms: i64,
) -> Result<StateRevision, StorageError> {
    let ingress = TransitionIngress {
            kind: TransitionIngressKind::ApplicationCommand,
            id: command_id.into_bytes(),
            fingerprint: fingerprint(command_fingerprint)?,
        };
    let receipt = TransitionCommit::open(path)?.commit_planned_with(
        ingress,
        UnixTimestampMilliseconds::new(observed_at_ms),
        |transaction| {
            crate::clip_store_read::require_clips_authoritative(transaction)?;
            let current = crate::library_store_clip_support::collection_revision(transaction)?;
            let committed = next_core_revision(transaction, "read clip core revision")?;
            let legacy = legacy_library_receipt(
                transaction,
                command_id,
                command_fingerprint,
                "read clip command receipt",
            )?;
            let invalid = pod0_domain::validate_clip(
                start_milliseconds,
                end_milliseconds,
                caption,
                frozen_transcript_text,
                source,
            )
            .is_err()
                || !crate::library_store_clip_support::clip_target_is_valid(
                    transaction,
                    episode_id,
                    podcast_id,
                )?
                || clip_exists(transaction, clip_id)?;
            let disposition = if legacy.is_some() {
                RequestDisposition::Duplicate
            } else if invalid {
                RequestDisposition::Rejected {
                    reason: RequestRejectionReason::Invalid,
                }
            } else {
                RequestDisposition::Accepted
            };
            plan_user_artifact_activity(UserArtifactActivityInput {
                command_id,
                subject: ActivitySubject::Clip { clip_id },
                episode_ids: vec![episode_id],
                current_revision: current,
                committed_revision: if disposition == RequestDisposition::Accepted {
                    committed
                } else {
                    current
                },
                transition: UserArtifactTransition::ClipChanged,
                disposition,
            })
            .map(|plan| {
                plan.map_mutation(|mutation| {
                    (mutation, committed, legacy.unwrap_or(current))
                })
            })
            .map_err(|_| StorageError::InvalidActivity)
        },
        |transaction, expected, mutation| match mutation {
            (UserArtifactMutation::Apply, committed, _) => {
                require_revision(transaction, expected)?;
                let revision = crate::library_store_clip_create::create_clip_in_transaction(
                    transaction,
                    command_id,
                    command_fingerprint,
                    clip_id,
                    episode_id,
                    podcast_id,
                    start_milliseconds,
                    end_milliseconds,
                    caption,
                    speaker_id,
                    frozen_transcript_text,
                    source,
                    observed_at_ms,
                )?;
                if revision != committed {
                    return Err(StorageError::RevisionConflict);
                }
                Ok(revision)
            }
            (UserArtifactMutation::None, _, return_revision) => {
                require_revision(transaction, expected)?;
                Ok(return_revision)
            }
        },
    )?;
    match receipt.disposition {
        RequestDisposition::Accepted | RequestDisposition::Duplicate => {
            Ok(receipt.committed_revision)
        }
        RequestDisposition::Rejected { .. } => Err(StorageError::InvalidClip),
        _ => Err(StorageError::InvalidActivity),
    }
}

fn clip_exists(connection: &rusqlite::Connection, clip_id: ClipId) -> Result<bool, StorageError> {
    match crate::library_store_clip_support::require_clip(connection, clip_id) {
        Ok(()) => Ok(true),
        Err(StorageError::EntityNotFound) => Ok(false),
        Err(error) => Err(error),
    }
}

fn require_revision(
    transaction: &rusqlite::Transaction<'_>,
    expected: StateRevision,
) -> Result<(), StorageError> {
    if crate::library_store_clip_support::collection_revision(transaction)? == expected {
        Ok(())
    } else {
        Err(StorageError::RevisionConflict)
    }
}
