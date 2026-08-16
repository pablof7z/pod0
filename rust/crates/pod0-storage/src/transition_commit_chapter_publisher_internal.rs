use std::cell::RefCell;

use pod0_application::{
    ActivityDomain, ActivitySubject, ChapterTransition, DomainTransitionKind,
    DurableEffectExecution, DurableExternalEffectRequest, ExternalEffectKind, InternalCommandKind,
    InternalCommandOwnerActivityInput, RequestDisposition, plan_internal_command_owner_activity,
};
use pod0_domain::{CancellationId, CommandId, ContentDigest, EpisodeId, StateRevision};
use sha2::{Digest as _, Sha256};

use super::TransitionCommit;
use crate::{
    PendingInternalCommand, PublisherChapterEnsureOutcome, PublisherChapterWorkflowRecord,
    PublisherChapterWorkflowState, StorageError, TransitionIngress, TransitionIngressKind,
};

#[allow(clippy::too_many_arguments)]
pub(crate) fn commit_publisher_chapter_internal_admission(
    path: &std::path::Path,
    command: PendingInternalCommand,
    source_url: &str,
    source_version: &str,
    cancellation_id: CancellationId,
    issued_revision: StateRevision,
    now_ms: i64,
    request_deadline_ms: i64,
    max_attempts: u16,
) -> Result<PublisherChapterEnsureOutcome, StorageError> {
    let episode_id = validate_command(&command)?;
    let command_id = CommandId::from_bytes(command.internal_command_id.into_bytes());
    let replaced = RefCell::new(None);
    let receipt = TransitionCommit::open(path)?.commit_planned_with(
        TransitionIngress {
            kind: TransitionIngressKind::InternalCommand,
            id: command.internal_command_id.into_bytes(),
            fingerprint: fingerprint(&command, source_url, source_version),
        },
        pod0_domain::UnixTimestampMilliseconds::new(now_ms),
        |transaction| {
            crate::chapter_workflow_store_support::require_current_source(
                transaction,
                episode_id,
                source_url,
            )?;
            let existing =
                crate::chapter_workflow_store_read::read_workflow(transaction, episode_id)?;
            let current = existing
                .as_ref()
                .map_or(StateRevision::INITIAL, |record| record.workflow_revision);
            let preserve = existing.as_ref().is_some_and(|record| {
                crate::chapter_workflow_store_support::should_preserve(
                    record,
                    source_version,
                    false,
                )
            });
            if preserve {
                return plan(
                    command.clone(),
                    command_id,
                    episode_id,
                    current,
                    false,
                    None,
                )
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
            let generation = existing.as_ref().map_or(Ok(1), |record| {
                record
                    .generation
                    .checked_add(1)
                    .ok_or(StorageError::ChapterWorkflowConflict)
            })?;
            let effect = (!adopts_current).then(|| {
                let deadline = pod0_domain::UnixTimestampMilliseconds::new(request_deadline_ms);
                DurableExternalEffectRequest {
                    kind: ExternalEffectKind::PublisherChapterProvider,
                    subject: ActivitySubject::Episode { episode_id },
                    episode_id: Some(episode_id),
                    not_before: None,
                    deadline_at: Some(deadline),
                    execution: DurableEffectExecution::PublisherChapter {
                        request: crate::chapter_effect_request::publisher_request(
                            crate::chapter_workflow_store_support::request_id_for_generation(
                                episode_id,
                                source_version,
                                generation,
                            ),
                            command_id,
                            cancellation_id,
                            issued_revision,
                            Some(deadline),
                            episode_id,
                            source_url.to_owned(),
                            None,
                        ),
                    },
                }
            });
            plan(
                command.clone(),
                command_id,
                episode_id,
                current,
                true,
                effect,
            )
            .map_err(|_| StorageError::InvalidActivity)
        },
        |transaction, expected, mutation| {
            if !mutation.changes_state {
                return Ok(expected);
            }
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
                false,
            )?;
            let revision = outcome_record(&outcome).workflow_revision;
            (revision.value == expected.value.saturating_add(1))
                .then_some(revision)
                .ok_or(StorageError::RevisionConflict)
        },
    )?;
    let record = crate::LibraryStore::open_authoritative(path)?
        .publisher_chapter_workflow(episode_id)?
        .ok_or(StorageError::ChapterWorkflowNotFound)?;
    if receipt.replayed || receipt.disposition != RequestDisposition::Accepted {
        Ok(PublisherChapterEnsureOutcome::Existing(record))
    } else {
        Ok(PublisherChapterEnsureOutcome::Requested {
            record,
            replaced: replaced.into_inner().map(Box::new),
        })
    }
}

fn plan(
    command: PendingInternalCommand,
    command_id: CommandId,
    episode_id: EpisodeId,
    current: StateRevision,
    changes: bool,
    effect: Option<DurableExternalEffectRequest>,
) -> Result<pod0_application::InternalCommandOwnerPlan, pod0_application::TransitionPlanError> {
    plan_internal_command_owner_activity(InternalCommandOwnerActivityInput {
        internal_command_id: command.internal_command_id,
        authorizing_activity_id: command.authorizing_activity_id,
        correlation_id: command.correlation_id,
        command_id,
        subject: ActivitySubject::Episode { episode_id },
        episode_id: Some(episode_id),
        current_revision: current,
        committed_revision: StateRevision::new(current.value + u64::from(changes)),
        disposition: if changes {
            RequestDisposition::Accepted
        } else {
            RequestDisposition::NoSemanticChange
        },
        transitions: changes
            .then_some((
                ActivitySubject::Episode { episode_id },
                DomainTransitionKind::Chapter(ChapterTransition::PublisherWorkflowStateChanged),
            ))
            .into_iter()
            .collect(),
        effects: effect.into_iter().collect(),
        internal_commands: Vec::new(),
    })
}

fn validate_command(command: &PendingInternalCommand) -> Result<EpisodeId, StorageError> {
    if command.request.target != ActivityDomain::Chapter
        || !matches!(
            command.request.kind,
            InternalCommandKind::EnsurePublisherChapters
        )
    {
        return Err(StorageError::InvalidActivity);
    }
    command
        .request
        .episode_id
        .ok_or(StorageError::InvalidActivity)
}

fn selected_publisher_matches(
    transaction: &rusqlite::Transaction<'_>,
    episode_id: EpisodeId,
    source_version: &str,
) -> Result<bool, StorageError> {
    Ok(
        crate::chapter_store_read_selection::read_selected_chapter_artifact(
            transaction,
            episode_id,
        )?
        .is_some_and(|selection| {
            selection.artifact.provenance.source == pod0_domain::ChapterArtifactSource::Publisher
                && selection.artifact.source_revision == source_version
        }),
    )
}

fn outcome_record(outcome: &PublisherChapterEnsureOutcome) -> &PublisherChapterWorkflowRecord {
    match outcome {
        PublisherChapterEnsureOutcome::Requested { record, .. }
        | PublisherChapterEnsureOutcome::Existing(record) => record,
    }
}

fn fingerprint(
    command: &PendingInternalCommand,
    source_url: &str,
    source_version: &str,
) -> ContentDigest {
    let mut hash = Sha256::new();
    hash.update(b"pod0/chapter/publisher-internal-admission/v1");
    hash.update(command.internal_command_id.into_bytes());
    hash.update(source_url.as_bytes());
    hash.update([0]);
    hash.update(source_version.as_bytes());
    ContentDigest::from_bytes(hash.finalize().into())
}
