use pod0_application::{
    ActivitySubject, CancellationEffectTarget, ChapterTransition, ChapterWorkflowMutation,
    DomainTransitionKind, WorkflowCancellationActivityInput, plan_workflow_cancellation_activity,
};
use pod0_domain::{CommandId, ContentDigest, EpisodeId, StateRevision, UnixTimestampMilliseconds};

use super::TransitionCommit;
use crate::{
    PublisherChapterWorkflowRecord, PublisherChapterWorkflowState, StorageError, TransitionIngress,
    TransitionIngressKind,
};

pub(crate) fn commit_publisher_chapter_cancellation(
    path: &std::path::Path,
    command_id: CommandId,
    fingerprint: ContentDigest,
    episode_id: EpisodeId,
    expected_revision: StateRevision,
    now_ms: i64,
) -> Result<PublisherChapterWorkflowRecord, StorageError> {
    TransitionCommit::open(path)?.commit_planned_with(
        TransitionIngress {
            kind: TransitionIngressKind::ApplicationCommand,
            id: command_id.into_bytes(),
            fingerprint,
        },
        UnixTimestampMilliseconds::new(now_ms),
        |transaction| {
            let current =
                crate::chapter_workflow_store_read::read_workflow(transaction, episode_id)?
                    .ok_or(StorageError::ChapterWorkflowNotFound)?;
            if current.workflow_revision != expected_revision
                || !matches!(
                    current.state,
                    PublisherChapterWorkflowState::Requested
                        | PublisherChapterWorkflowState::RetryScheduled
                )
            {
                return Err(StorageError::ChapterWorkflowConflict);
            }
            plan_workflow_cancellation_activity(WorkflowCancellationActivityInput {
                command_id,
                episode_id,
                subject: ActivitySubject::Episode { episode_id },
                current_revision: current.workflow_revision,
                transition: DomainTransitionKind::Chapter(
                    ChapterTransition::PublisherWorkflowStateChanged,
                ),
                target: current.request_id.map(|host_request_id| CancellationEffectTarget {
                    subject: ActivitySubject::Episode { episode_id },
                    episode_id: Some(episode_id),
                    host_request_id,
                    cancellation_id: current.cancellation_id,
                }),
            })
            .map(|plan| plan.map_mutation(|()| ChapterWorkflowMutation::Apply))
            .map_err(|_| StorageError::InvalidActivity)
        },
        |transaction, expected, mutation| {
            if mutation != ChapterWorkflowMutation::Apply {
                return Err(StorageError::InvalidActivity);
            }
            let current =
                crate::chapter_workflow_store_read::read_workflow(transaction, episode_id)?
                    .ok_or(StorageError::ChapterWorkflowNotFound)?;
            let revision = crate::LibraryStore::apply_publisher_chapter_cancellation(
                transaction,
                episode_id,
                expected,
                now_ms,
            )?
            .workflow_revision;
            retire_chapter_effects(transaction, current.command_id)?;
            Ok(revision)
        },
    )?;
    crate::LibraryStore::open_authoritative(path)?
        .publisher_chapter_workflow(episode_id)?
        .ok_or(StorageError::ChapterWorkflowNotFound)
}

fn retire_chapter_effects(
    transaction: &rusqlite::Transaction<'_>,
    authorizing_command_id: CommandId,
) -> Result<(), StorageError> {
    transaction
        .execute(
            "UPDATE pod0_effect_attempts SET state_code=4 WHERE intent_id IN (
             SELECT i.intent_id FROM pod0_effect_intents i
             JOIN pod0_activity_facts f ON f.activity_id=i.authorizing_activity_id
             WHERE f.command_id=?1 AND i.effect_kind_code=4 AND i.state_code IN(1,2))
             AND state_code=1",
            [authorizing_command_id.into_bytes().as_slice()],
        )
        .map_err(|error| StorageError::sqlite("retire chapter effect attempts", error))?;
    transaction
        .execute(
            "UPDATE pod0_effect_intents SET state_code=4 WHERE effect_kind_code=4
             AND state_code IN(1,2) AND authorizing_activity_id IN (
             SELECT activity_id FROM pod0_activity_facts WHERE command_id=?1)",
            [authorizing_command_id.into_bytes().as_slice()],
        )
        .map_err(|error| StorageError::sqlite("retire chapter effect intents", error))?;
    Ok(())
}
