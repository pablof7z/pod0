use std::cell::RefCell;

use pod0_application::{
    ActivityActor, ActivityOrigin, ChapterTransition, ChapterWorkflowActivityInput,
    ChapterWorkflowEffectAuthorization, ChapterWorkflowExecution, ChapterWorkflowMutation, ExternalEffectKind,
    RequestDisposition, plan_chapter_workflow_activity,
};
use pod0_domain::{
    CancellationId, CommandId, ContentDigest, EpisodeId, StateRevision, UnixTimestampMilliseconds,
};
use sha2::{Digest as _, Sha256};

use super::TransitionCommit;
use crate::{
    PublisherChapterEnsureOutcome, PublisherChapterWorkflowRecord, PublisherChapterWorkflowState,
    StorageError, TransitionIngress, TransitionIngressKind,
};

#[allow(clippy::too_many_arguments)]
pub(crate) fn commit_publisher_chapter_admission(
    path: &std::path::Path,
    episode_id: EpisodeId,
    source_url: &str,
    source_version: &str,
    command_id: CommandId,
    cancellation_id: CancellationId,
    issued_revision: StateRevision,
    now_ms: i64,
    request_deadline_ms: i64,
    max_attempts: u16,
    force_retry: bool,
    recovery: bool,
) -> Result<PublisherChapterEnsureOutcome, StorageError> {
    let replaced = RefCell::new(None);
    let recovery_identity = recovery_identity(command_id, source_version);
    let receipt = TransitionCommit::open(path)?.commit_planned_with(
        TransitionIngress {
            kind: if recovery {
                TransitionIngressKind::Recovery
            } else {
                TransitionIngressKind::ApplicationCommand
            },
            id: if recovery {
                recovery_identity.into_bytes()
            } else {
                command_id.into_bytes()
            },
            fingerprint: fingerprint(episode_id, source_url, source_version, force_retry),
        },
        UnixTimestampMilliseconds::new(now_ms),
        |transaction| {
            crate::chapter_workflow_store_support::require_current_source(
                transaction,
                episode_id,
                source_url,
            )?;
            let existing =
                crate::chapter_workflow_store_read::read_workflow(transaction, episode_id)?;
            let current_revision = existing
                .as_ref()
                .map_or(StateRevision::INITIAL, |record| record.workflow_revision);
            if let Some(record) = existing.as_ref()
                && crate::chapter_workflow_store_support::should_preserve(
                    record,
                    source_version,
                    force_retry,
                )
            {
                return plan_chapter_workflow_activity(ChapterWorkflowActivityInput {
                    identity_command_id: if recovery {
                        recovery_identity
                    } else {
                        command_id
                    },
                    command_id,
                    episode_id,
                    current_revision,
                    disposition: if record.state == PublisherChapterWorkflowState::Succeeded {
                        RequestDisposition::AlreadyComplete
                    } else {
                        RequestDisposition::NoSemanticChange
                    },
                    transition: None,
                    effect: None,
                    effect_kind: ExternalEffectKind::PublisherChapterProvider,
                    actor: if recovery {
                        ActivityActor::Recovery
                    } else {
                        ActivityActor::User
                    },
                    origin: if recovery {
                        ActivityOrigin::Recovery
                    } else {
                        ActivityOrigin::UserInterface
                    },
                })
                .map_err(|_| StorageError::InvalidActivity);
            }
            *replaced.borrow_mut() = existing.clone().filter(|record| {
                matches!(
                    record.state,
                    PublisherChapterWorkflowState::Requested
                        | PublisherChapterWorkflowState::RetryScheduled
                )
            });
            let adopts_current = existing.is_none()
                && selected_publisher_matches(transaction, episode_id, source_version)?;
            let next_generation = existing.as_ref().map_or(Ok(1), |record| {
                record.generation.checked_add(1).ok_or(StorageError::ChapterWorkflowConflict)
            })?;
            let effect = (!adopts_current).then(|| {
                let deadline = UnixTimestampMilliseconds::new(request_deadline_ms);
                ChapterWorkflowEffectAuthorization {
                    not_before: None,
                    deadline_at: Some(deadline),
                    execution: ChapterWorkflowExecution::Publisher(
                        crate::chapter_effect_request::publisher_request(
                            crate::chapter_workflow_store_support::request_id_for_generation(
                                episode_id,
                                source_version,
                                next_generation,
                            ),
                            command_id,
                            cancellation_id,
                            issued_revision,
                            Some(deadline),
                            episode_id,
                            source_url.to_owned(),
                            None,
                        ),
                    ),
                }
            });
            plan_chapter_workflow_activity(ChapterWorkflowActivityInput {
                identity_command_id: if recovery {
                    recovery_identity
                } else {
                    command_id
                },
                command_id,
                episode_id,
                current_revision,
                disposition: RequestDisposition::Accepted,
                transition: Some(ChapterTransition::PublisherWorkflowStateChanged),
                effect,
                effect_kind: ExternalEffectKind::PublisherChapterProvider,
                actor: if recovery {
                    ActivityActor::Recovery
                } else {
                    ActivityActor::User
                },
                origin: if recovery {
                    ActivityOrigin::Recovery
                } else {
                    ActivityOrigin::UserInterface
                },
            })
            .map_err(|_| StorageError::InvalidActivity)
        },
        |transaction, expected, mutation| match mutation {
            ChapterWorkflowMutation::Apply => {
                let outcome = crate::LibraryStore::apply_publisher_chapter_ensure(
                    transaction,
                    episode_id,
                    source_url,
                    source_version,
                    command_id,
                    cancellation_id,
                    issued_revision,
                    now_ms,
                    request_deadline_ms,
                    max_attempts,
                    force_retry,
                )?;
                let record = outcome_record(&outcome);
                if record.workflow_revision.value != expected.value.saturating_add(1) {
                    return Err(StorageError::RevisionConflict);
                }
                Ok(record.workflow_revision)
            }
            ChapterWorkflowMutation::RecordNoChange => {
                let current =
                    crate::chapter_workflow_store_read::read_workflow(transaction, episode_id)?
                        .ok_or(StorageError::ChapterWorkflowNotFound)?;
                if current.workflow_revision != expected {
                    return Err(StorageError::RevisionConflict);
                }
                Ok(expected)
            }
        },
    )?;
    let record = crate::LibraryStore::open_authoritative(path)?
        .publisher_chapter_workflow(episode_id)?
        .ok_or(StorageError::ChapterWorkflowNotFound)?;
    if receipt.replayed
        || receipt.disposition != RequestDisposition::Accepted
        || record.state == PublisherChapterWorkflowState::Succeeded
    {
        Ok(PublisherChapterEnsureOutcome::Existing(record))
    } else {
        Ok(PublisherChapterEnsureOutcome::Requested {
            record,
            replaced: replaced.into_inner().map(Box::new),
        })
    }
}

fn selected_publisher_matches(
    transaction: &rusqlite::Transaction<'_>,
    episode_id: EpisodeId,
    source_version: &str,
) -> Result<bool, StorageError> {
    let selected = crate::chapter_store_read_selection::read_selected_chapter_artifact(
        transaction,
        episode_id,
    )?;
    Ok(selected.is_some_and(|selection| {
        selection.artifact.provenance.source == pod0_domain::ChapterArtifactSource::Publisher
            && selection.artifact.source_revision == source_version
    }))
}

fn outcome_record(outcome: &PublisherChapterEnsureOutcome) -> &PublisherChapterWorkflowRecord {
    match outcome {
        PublisherChapterEnsureOutcome::Requested { record, .. }
        | PublisherChapterEnsureOutcome::Existing(record) => record,
    }
}

fn fingerprint(
    episode_id: EpisodeId,
    source_url: &str,
    source_version: &str,
    force_retry: bool,
) -> ContentDigest {
    let mut hash = Sha256::new();
    hash.update(b"pod0/chapter/publisher-admission/v1");
    hash.update(episode_id.into_bytes());
    hash.update(source_url.as_bytes());
    hash.update([0]);
    hash.update(source_version.as_bytes());
    hash.update([u8::from(force_retry)]);
    ContentDigest::from_bytes(hash.finalize().into())
}

fn recovery_identity(command_id: CommandId, source_version: &str) -> CommandId {
    let mut hash = Sha256::new();
    hash.update(b"pod0/chapter/publisher-recovery/v1");
    hash.update(command_id.into_bytes());
    hash.update(source_version.as_bytes());
    let digest: [u8; 32] = hash.finalize().into();
    CommandId::from_bytes(digest[..16].try_into().expect("fixed digest prefix"))
}
