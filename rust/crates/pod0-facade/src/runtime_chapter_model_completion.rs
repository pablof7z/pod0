use pod0_application::{
    ChapterModelFailureEvidence, ChapterModelObservationMode, ChapterModelRetryDisposition,
    ChapterObservationProjection, ModelChapterObservation, classify_chapter_model_failure,
    qualify_model_chapter_observation,
};
use pod0_storage::{
    ModelChapterFailureDisposition, ModelChapterSuccessInput, ModelChapterWorkflowRecord,
    ModelChapterWorkflowState, StorageError,
};

use crate::runtime_state::FacadeState;

impl FacadeState {
    /// Finishes already-durable raw evidence. Returning false leaves the
    /// CompletionObserved row available for restart recovery.
    pub(super) fn resume_staged_model_completion(
        &mut self,
        request_id: pod0_domain::HostRequestId,
    ) -> bool {
        let Some(store) = self.store.clone() else {
            return false;
        };
        let completion = match store.model_chapter_completion(request_id) {
            Ok(Some(value)) => value,
            _ => return false,
        };
        let record = match store.model_chapter_workflow(completion.episode_id) {
            Ok(Some(value)) if value.request_id == Some(request_id) => value,
            _ => return false,
        };
        if record.state == ModelChapterWorkflowState::Succeeded {
            return true;
        }
        if record.state != ModelChapterWorkflowState::CompletionObserved {
            return false;
        }
        let active = match record.active_request.as_ref() {
            Some(value) => value,
            None => return false,
        };
        if !self.selected_transcript_is_current(record.episode_id, active) {
            return self.fail_staged_model_completion(
                &record,
                ChapterModelFailureEvidence::StaleTranscript,
            );
        }
        let mode = match self.model_observation_mode(active) {
            Ok(value) => value,
            Err(evidence) => return self.fail_staged_model_completion(&record, evidence),
        };
        let Some(episode) = self
            .listening
            .episodes
            .iter()
            .find(|episode| episode.episode_id == completion.episode_id)
        else {
            return self.fail_staged_model_completion(
                &record,
                ChapterModelFailureEvidence::InvalidRequest,
            );
        };
        let projection = qualify_model_chapter_observation(ModelChapterObservation {
            episode_id: completion.episode_id,
            podcast_id: episode.podcast_id,
            format_version: active.format_version,
            requested_transcript_version_id: active.requested_transcript_version_id,
            requested_transcript_content_digest: active.requested_transcript_digest,
            selected_transcript_version_id: active.selected_transcript_version_id,
            selected_transcript_content_digest: active.selected_transcript_digest,
            policy_version: active.policy_version,
            source_version: active.source_version.clone(),
            provider: completion.provider,
            model: completion.model,
            completion_digest: completion.completion_digest,
            completion: completion.completion,
            generated_at: pod0_domain::UnixTimestampMilliseconds::new(completion.generated_at_ms),
            duration_milliseconds: active.duration_ms,
            mode,
        });
        let ChapterObservationProjection::Qualified { artifact, .. } = projection else {
            let ChapterObservationProjection::Rejected { reason } = projection else {
                unreachable!()
            };
            return self.fail_staged_model_completion(
                &record,
                ChapterModelFailureEvidence::Qualification { reason },
            );
        };
        match store.complete_model_chapter_workflow(ModelChapterSuccessInput {
            episode_id: record.episode_id,
            request_id,
            generation: record.generation,
            submission_fence_id: record
                .submission_fence_id
                .expect("completion-observed workflow has a fence"),
            artifact,
            completed_at_ms: self.now().value,
        }) {
            Ok(_) => {
                let _ = self.reload_listening();
                self.advance_revision();
                self.succeed(record.command_id, None);
                true
            }
            Err(StorageError::ChapterRevisionConflict) => self.fail_staged_model_completion(
                &record,
                ChapterModelFailureEvidence::SelectionChanged,
            ),
            Err(StorageError::InvalidChapterArtifact) => self.fail_staged_model_completion(
                &record,
                ChapterModelFailureEvidence::StaleTranscript,
            ),
            Err(_) => false,
        }
    }

    fn selected_transcript_is_current(
        &self,
        episode_id: pod0_domain::EpisodeId,
        request: &pod0_storage::StoredModelChapterRequest,
    ) -> bool {
        self.transcript_store
            .as_ref()
            .and_then(|store| store.selected_artifact(episode_id).ok())
            .flatten()
            .is_some_and(|artifact| {
                artifact.transcript_version_id == request.selected_transcript_version_id
                    && artifact.content_digest == request.selected_transcript_digest
            })
    }

    fn model_observation_mode(
        &self,
        request: &pod0_storage::StoredModelChapterRequest,
    ) -> Result<ChapterModelObservationMode, ChapterModelFailureEvidence> {
        match request.mode {
            pod0_storage::ModelChapterWorkflowMode::Generate => {
                Ok(ChapterModelObservationMode::Generate)
            }
            pod0_storage::ModelChapterWorkflowMode::Enrich => {
                let base_id = request
                    .base_artifact_id
                    .ok_or(ChapterModelFailureEvidence::StalePublisherBase)?;
                let base = self
                    .store
                    .as_ref()
                    .and_then(|store| store.chapter_artifact(base_id).ok())
                    .flatten()
                    .filter(|artifact| {
                        Some(artifact.integrity_digest) == request.base_integrity_digest
                    })
                    .ok_or(ChapterModelFailureEvidence::StalePublisherBase)?;
                Ok(ChapterModelObservationMode::Enrich {
                    publisher_artifact: base.as_input(),
                })
            }
        }
    }

    fn fail_staged_model_completion(
        &mut self,
        record: &ModelChapterWorkflowRecord,
        evidence: ChapterModelFailureEvidence,
    ) -> bool {
        let classification = classify_chapter_model_failure(evidence);
        let disposition = if classification.retry == ChapterModelRetryDisposition::Replan {
            ModelChapterFailureDisposition::Replan
        } else {
            ModelChapterFailureDisposition::Block
        };
        self.commit_model_chapter_failure(record, evidence, disposition, None)
    }
}
