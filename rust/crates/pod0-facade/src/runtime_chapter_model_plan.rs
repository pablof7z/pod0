use pod0_application::{
    ChapterModelEpisodeInput, ChapterModelPlan, ChapterModelPlanInput, ChapterModelTranscriptInput,
    ChapterModelTranscriptSegmentInput,
};
use pod0_domain::{ChapterArtifactInput, ChapterArtifactSource, EpisodeId, StateRevision};

use crate::Pod0Facade;
use crate::runtime_state::FacadeState;

impl Pod0Facade {
    pub(super) fn chapter_model_plan(
        &self,
        episode_id: EpisodeId,
        configured_model: String,
    ) -> ChapterModelPlan {
        self.state()
            .chapter_model_plan(episode_id, configured_model)
    }
}

impl FacadeState {
    pub(super) fn chapter_model_plan(
        &self,
        episode_id: EpisodeId,
        configured_model: String,
    ) -> ChapterModelPlan {
        let Some(episode) = self
            .listening
            .episodes
            .iter()
            .find(|episode| episode.episode_id == episode_id)
        else {
            return ChapterModelPlan::EpisodeUnavailable;
        };
        let Some(transcript_store) = self.transcript_store.as_ref() else {
            return ChapterModelPlan::CoreUnavailable;
        };
        let transcript = match transcript_store.selected_artifact(episode_id) {
            Ok(Some(artifact)) => artifact,
            Ok(None) => return ChapterModelPlan::TranscriptUnavailable,
            Err(_) => return ChapterModelPlan::CoreUnavailable,
        };
        let selected_chapter = match self.store.as_ref() {
            Some(store) => match store.selected_chapter_artifact(episode_id) {
                Ok(value) => value,
                Err(_) => return ChapterModelPlan::CoreUnavailable,
            },
            None => return ChapterModelPlan::CoreUnavailable,
        };
        let publisher_base = match self.publisher_base_for_model_plan(
            episode_id,
            selected_chapter
                .as_ref()
                .map(|selection| &selection.artifact),
        ) {
            Ok(value) => value,
            Err(()) => return ChapterModelPlan::CoreUnavailable,
        };
        let selected_transcript = ChapterModelTranscriptInput {
            transcript_version_id: transcript.transcript_version_id,
            transcript_content_digest: transcript.content_digest,
            segments: transcript
                .segments
                .iter()
                .map(|segment| ChapterModelTranscriptSegmentInput {
                    start_seconds: segment.start_milliseconds as f64 / 1_000.0,
                    text: segment.text.clone(),
                })
                .collect(),
        };
        pod0_application::plan_chapter_model_request(ChapterModelPlanInput {
            episode: ChapterModelEpisodeInput {
                episode_id,
                podcast_id: episode.podcast_id,
                title: episode.title.clone(),
                description: episode.description.clone(),
                duration_seconds: episode
                    .duration_milliseconds
                    .map(|value| value as f64 / 1_000.0),
            },
            requested_transcript_version_id: transcript.transcript_version_id,
            requested_transcript_content_digest: transcript.content_digest,
            selected_transcript: Some(selected_transcript),
            selected_chapter_artifact: selected_chapter
                .as_ref()
                .map(|selection| selection.artifact.as_input()),
            publisher_base_artifact: publisher_base,
            expected_chapter_selection_revision: selected_chapter
                .as_ref()
                .map_or(StateRevision::INITIAL, |selection| {
                    selection.selection_revision
                }),
            configured_model,
        })
    }

    fn publisher_base_for_model_plan(
        &self,
        episode_id: EpisodeId,
        selected: Option<&pod0_domain::ChapterArtifact>,
    ) -> Result<Option<ChapterArtifactInput>, ()> {
        let Some(selected) = selected else {
            return Ok(None);
        };
        match selected.provenance.source {
            ChapterArtifactSource::Publisher => Ok(Some(selected.as_input())),
            ChapterArtifactSource::PublisherEnriched => {
                let store = self.store.as_ref().ok_or(())?;
                let artifact_id = store
                    .publisher_chapter_workflow(episode_id)
                    .map_err(|_| ())?
                    .and_then(|record| record.selected_artifact_id)
                    .ok_or(())?;
                let artifact = store
                    .chapter_artifact(artifact_id)
                    .map_err(|_| ())?
                    .ok_or(())?;
                (artifact.provenance.source == ChapterArtifactSource::Publisher
                    && artifact.episode_id == episode_id)
                    .then(|| artifact.as_input())
                    .ok_or(())
                    .map(Some)
            }
            _ => Ok(None),
        }
    }
}
