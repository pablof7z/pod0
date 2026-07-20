use pod0_domain::{ChapterArtifact, ChapterArtifactInput, ChapterArtifactSource, StateRevision};

use super::model::{
    ModelChapterWorkflowRecord, ModelChapterWorkflowState, StoredModelChapterRequest,
};
use super::persist::persist_workflow;
use super::read::{read_completion, read_workflow};
use crate::library_store_chapters::commit_and_select_chapter_in_transaction;
use crate::{ChapterCommitStorageReceipt, LibraryStore, StorageError};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelChapterSuccessInput {
    pub episode_id: pod0_domain::EpisodeId,
    pub request_id: pod0_domain::HostRequestId,
    pub generation: u64,
    pub submission_fence_id: pod0_domain::ChapterModelSubmissionFenceId,
    pub artifact: ChapterArtifactInput,
    pub completed_at_ms: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelChapterSuccessReceipt {
    pub workflow: ModelChapterWorkflowRecord,
    pub chapter: ChapterCommitStorageReceipt,
}

impl LibraryStore {
    pub fn complete_model_chapter_workflow(
        &self,
        input: ModelChapterSuccessInput,
    ) -> Result<ModelChapterSuccessReceipt, StorageError> {
        if input.completed_at_ms < 0 {
            return Err(StorageError::ChapterWorkflowConflict);
        }
        let artifact = ChapterArtifact::seal(input.artifact)
            .map_err(|_| StorageError::InvalidChapterArtifact)?;
        self.write(|transaction| {
            let mut workflow = read_workflow(transaction, input.episode_id)?
                .ok_or(StorageError::ChapterWorkflowNotFound)?;
            if workflow.request_id != Some(input.request_id)
                || workflow.generation != input.generation
                || workflow.submission_fence_id != Some(input.submission_fence_id)
                || !matches!(
                    workflow.state,
                    ModelChapterWorkflowState::CompletionObserved
                        | ModelChapterWorkflowState::Succeeded
                )
            {
                return Err(StorageError::ChapterWorkflowConflict);
            }
            let request = workflow
                .active_request
                .as_ref()
                .ok_or(StorageError::ChapterWorkflowConflict)?;
            let completion = read_completion(transaction, input.request_id)?
                .ok_or(StorageError::ChapterWorkflowConflict)?;
            validate_artifact(&artifact, request, &completion)?;
            let chapter = commit_and_select_chapter_in_transaction(
                transaction,
                workflow.command_id,
                request.expected_selection_revision,
                &artifact,
                input.completed_at_ms,
                || Ok(()),
            )?;
            if workflow.state != ModelChapterWorkflowState::Succeeded {
                workflow.state = ModelChapterWorkflowState::Succeeded;
                workflow.workflow_revision = next_revision(workflow.workflow_revision)?;
                workflow.selected_artifact_id = Some(chapter.artifact_id);
                workflow.deadline_at_ms = None;
                workflow.not_before_ms = None;
                workflow.failure_code = None;
                workflow.failure_detail = None;
                workflow.updated_at_ms = input.completed_at_ms;
                persist_workflow(transaction, &workflow)?;
            } else if workflow.selected_artifact_id != Some(chapter.artifact_id) {
                return Err(StorageError::ChapterWorkflowConflict);
            }
            let workflow = read_workflow(transaction, input.episode_id)?
                .ok_or(StorageError::ChapterWorkflowNotFound)?;
            Ok(ModelChapterSuccessReceipt { workflow, chapter })
        })
    }
}

fn validate_artifact(
    artifact: &ChapterArtifact,
    request: &StoredModelChapterRequest,
    completion: &super::inputs::ModelChapterCompletionRecord,
) -> Result<(), StorageError> {
    let provenance = &artifact.provenance;
    if artifact.episode_id != completion.episode_id
        || provenance.source != request.expected_artifact_source
        || provenance.provider.as_deref() != Some(request.provider.as_str())
        || provenance.model.as_deref() != Some(request.model.as_str())
        || provenance.policy_version != request.policy_version
        || provenance.source_payload_digest != completion.completion_digest
        || provenance.transcript_version_id != Some(request.selected_transcript_version_id)
        || provenance.transcript_content_digest != Some(request.selected_transcript_digest)
        || artifact.duration_milliseconds != request.duration_ms
        || artifact.generated_at.value != completion.generated_at_ms
        || !valid_mode_source(request.mode, provenance.source)
    {
        return Err(StorageError::ChapterWorkflowConflict);
    }
    Ok(())
}

fn valid_mode_source(
    mode: super::model::ModelChapterWorkflowMode,
    source: ChapterArtifactSource,
) -> bool {
    matches!(
        (mode, source),
        (
            super::model::ModelChapterWorkflowMode::Generate,
            ChapterArtifactSource::Generated
        ) | (
            super::model::ModelChapterWorkflowMode::Enrich,
            ChapterArtifactSource::PublisherEnriched
        )
    )
}

fn next_revision(current: StateRevision) -> Result<StateRevision, StorageError> {
    current
        .value
        .checked_add(1)
        .map(StateRevision::new)
        .ok_or(StorageError::ChapterWorkflowConflict)
}
