use pod0_application::{
    ActivitySubject, CancellationEffectTarget, ChapterTransition, ChapterWorkflowMutation,
    DomainTransitionKind, WorkflowCancellationActivityInput, plan_workflow_cancellation_activity,
};
use pod0_domain::{CommandId, ContentDigest, EpisodeId, StateRevision, UnixTimestampMilliseconds};

use super::TransitionCommit;
use crate::{
    LibraryStore, ModelChapterWorkflowRecord, ModelChapterWorkflowState, StorageError,
    TransitionIngress, TransitionIngressKind,
};

impl LibraryStore {
    pub fn cancel_model_chapter_command(
        &self,
        command_id: CommandId,
        fingerprint: ContentDigest,
        episode_id: EpisodeId,
        expected_revision: StateRevision,
        observed_at_ms: i64,
    ) -> Result<ModelChapterWorkflowRecord, StorageError> {
        TransitionCommit::open(self.path())?.commit_planned_with(
            TransitionIngress {
                kind: TransitionIngressKind::ApplicationCommand,
                id: command_id.into_bytes(),
                fingerprint,
            },
            UnixTimestampMilliseconds::new(observed_at_ms),
            |transaction| {
                let current =
                    crate::model_chapter_workflow::read::read_workflow(transaction, episode_id)?
                        .ok_or(StorageError::ChapterWorkflowNotFound)?;
                if current.workflow_revision != expected_revision
                    || !matches!(
                        current.state,
                        ModelChapterWorkflowState::AwaitingTranscript
                            | ModelChapterWorkflowState::AwaitingPublisher
                            | ModelChapterWorkflowState::Requested
                            | ModelChapterWorkflowState::RetryScheduled
                            | ModelChapterWorkflowState::Ambiguous
                            | ModelChapterWorkflowState::Blocked
                            | ModelChapterWorkflowState::Failed
                            | ModelChapterWorkflowState::SubmissionAuthorized
                            | ModelChapterWorkflowState::ProviderAccepted
                            | ModelChapterWorkflowState::CompletionObserved
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
                        ChapterTransition::ModelWorkflowStateChanged,
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
                let updated = LibraryStore::apply_model_chapter_cancellation(
                    transaction,
                    episode_id,
                    expected,
                    observed_at_ms,
                )?;
                retire_model_effects(transaction, episode_id)?;
                Ok(updated.workflow_revision)
            },
        )?;
        self.model_chapter_workflow(episode_id)?
            .ok_or(StorageError::ChapterWorkflowNotFound)
    }
}

pub(super) fn retire_model_effects(
    transaction: &rusqlite::Transaction<'_>,
    episode_id: EpisodeId,
) -> Result<(), StorageError> {
    transaction
        .execute(
            "UPDATE pod0_effect_attempts SET state_code=4 WHERE intent_id IN (
             SELECT intent_id FROM pod0_effect_intents WHERE episode_id=?1
             AND effect_kind_code=4
             AND json_extract(request_json,'$.kind')='ModelChapterProvider'
             AND state_code IN(1,2)) AND state_code=1",
            [episode_id.into_bytes().as_slice()],
        )
        .map_err(|error| StorageError::sqlite("retire model chapter attempts", error))?;
    transaction
        .execute(
            "UPDATE pod0_effect_intents SET state_code=4 WHERE episode_id=?1
             AND effect_kind_code=4
             AND json_extract(request_json,'$.kind')='ModelChapterProvider'
             AND state_code IN(1,2)",
            [episode_id.into_bytes().as_slice()],
        )
        .map_err(|error| StorageError::sqlite("retire model chapter intents", error))?;
    Ok(())
}
