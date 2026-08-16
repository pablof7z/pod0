use pod0_application::{
    ActivitySubject, ChapterFinalizationActivityInput, ChapterRecordedTransition,
    ChapterTransition, InternalCommandKind, plan_chapter_finalization_activity,
};
use pod0_domain::{
    ChapterArtifact, ContentDigest, EpisodeId, StateRevision, UnixTimestampMilliseconds,
};
use sha2::{Digest as _, Sha256};

use super::TransitionCommit;
use crate::{
    LibraryStore, ModelChapterFinalizationAction, ModelChapterWorkflowRecord,
    PendingInternalCommand, StorageError, TransitionIngress, TransitionIngressKind,
};

impl LibraryStore {
    pub fn finalize_model_chapters_from_internal_command(
        &self,
        command: PendingInternalCommand,
        action: ModelChapterFinalizationAction,
        committed_at: UnixTimestampMilliseconds,
    ) -> Result<ModelChapterWorkflowRecord, StorageError> {
        let (episode_id, request_id) = validate_command(&command, &action)?;
        let fingerprint = fingerprint(&action)?;
        let mutation_action = action.clone();
        TransitionCommit::open(self.path())?.commit_planned_with(
            TransitionIngress {
                kind: TransitionIngressKind::InternalCommand,
                id: command.internal_command_id.into_bytes(),
                fingerprint,
            },
            committed_at,
            |transaction| {
                let current =
                    crate::model_chapter_workflow::read::read_workflow(transaction, episode_id)?
                        .ok_or(StorageError::ChapterWorkflowNotFound)?;
                if current.request_id != Some(request_id) {
                    return Err(StorageError::ChapterWorkflowConflict);
                }
                let mut transitions = vec![ChapterRecordedTransition {
                    kind: ChapterTransition::ModelWorkflowStateChanged,
                    previous_revision: current.workflow_revision,
                    committed_revision: next_revision(current.workflow_revision)?,
                }];
                if let ModelChapterFinalizationAction::Success(input) = &action {
                    let artifact = ChapterArtifact::seal(input.artifact.clone())
                        .map_err(|_| StorageError::InvalidChapterArtifact)?;
                    let selected = crate::chapter_workflow_store_support::selected_chapter(
                        transaction,
                        episode_id,
                    )?;
                    if selected.map(|item| item.0) != Some(artifact.artifact_id) {
                        let previous = selected.map_or(StateRevision::INITIAL, |item| item.1);
                        let committed = next_revision(previous)?;
                        transitions.extend([
                            ChapterRecordedTransition {
                                kind: ChapterTransition::ArtifactAdopted,
                                previous_revision: previous,
                                committed_revision: committed,
                            },
                            ChapterRecordedTransition {
                                kind: ChapterTransition::SelectionChanged,
                                previous_revision: previous,
                                committed_revision: committed,
                            },
                        ]);
                    }
                }
                plan_chapter_finalization_activity(ChapterFinalizationActivityInput {
                    internal_command_id: command.internal_command_id,
                    authorizing_activity_id: command.authorizing_activity_id,
                    correlation_id: command.correlation_id,
                    command_id: current.command_id,
                    request_id,
                    episode_id,
                    current_workflow_revision: current.workflow_revision,
                    transitions,
                })
                .map_err(|_| StorageError::InvalidActivity)
            },
            |transaction, expected, ()| {
                let updated = match mutation_action {
                    ModelChapterFinalizationAction::Success(input) => {
                        LibraryStore::apply_model_chapter_success(transaction, input)?.workflow
                    }
                    ModelChapterFinalizationAction::Failure(input) => {
                        LibraryStore::apply_model_chapter_failure(transaction, input)?
                    }
                };
                if updated.workflow_revision.value != expected.value.saturating_add(1) {
                    return Err(StorageError::RevisionConflict);
                }
                Ok(updated.workflow_revision)
            },
        )?;
        self.model_chapter_workflow(episode_id)?
            .ok_or(StorageError::ChapterWorkflowNotFound)
    }
}

fn validate_command(
    command: &PendingInternalCommand,
    action: &ModelChapterFinalizationAction,
) -> Result<(EpisodeId, pod0_domain::HostRequestId), StorageError> {
    let InternalCommandKind::FinalizeModelChapters { request_id } = command.request.kind else {
        return Err(StorageError::InvalidActivity);
    };
    let ActivitySubject::Episode { episode_id } = command.request.subject else {
        return Err(StorageError::InvalidActivity);
    };
    if command.request.target != pod0_application::ActivityDomain::Chapter
        || command.request.episode_id != Some(episode_id)
        || action_episode(action) != episode_id
        || action_request(action) != request_id
    {
        return Err(StorageError::InvalidActivity);
    }
    Ok((episode_id, request_id))
}

fn action_episode(action: &ModelChapterFinalizationAction) -> EpisodeId {
    match action {
        ModelChapterFinalizationAction::Success(input) => input.episode_id,
        ModelChapterFinalizationAction::Failure(input) => input.episode_id,
    }
}

fn action_request(action: &ModelChapterFinalizationAction) -> pod0_domain::HostRequestId {
    match action {
        ModelChapterFinalizationAction::Success(input) => input.request_id,
        ModelChapterFinalizationAction::Failure(input) => input.request_id,
    }
}

fn fingerprint(action: &ModelChapterFinalizationAction) -> Result<ContentDigest, StorageError> {
    let mut hash = Sha256::new();
    hash.update(b"pod0/chapter/model-finalization/v1");
    match action {
        ModelChapterFinalizationAction::Success(input) => {
            hash.update([1]);
            let artifact = ChapterArtifact::seal(input.artifact.clone())
                .map_err(|_| StorageError::InvalidChapterArtifact)?;
            hash.update(artifact.integrity_digest.into_bytes());
        }
        ModelChapterFinalizationAction::Failure(input) => {
            hash.update([2]);
            hash.update(input.failure_code.as_bytes());
            hash.update([0]);
            hash.update(
                input
                    .failure_detail
                    .as_deref()
                    .unwrap_or_default()
                    .as_bytes(),
            );
        }
    }
    Ok(ContentDigest::from_bytes(hash.finalize().into()))
}

fn next_revision(revision: StateRevision) -> Result<StateRevision, StorageError> {
    revision
        .value
        .checked_add(1)
        .map(StateRevision::new)
        .ok_or(StorageError::InvalidActivity)
}
