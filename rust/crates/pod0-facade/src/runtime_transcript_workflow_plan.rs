use pod0_application::{
    CommittedTranscriptGeneration, TranscriptEvidenceDecision, TranscriptGenerationDecision,
    TranscriptWorkflowConfiguration, TranscriptWorkflowOrigin, TranscriptWorkflowPlan,
    TranscriptWorkflowPlanInput, TranscriptWorkflowRequest, plan_transcript_workflow,
    transcript_evidence_input_version, transcript_source_revision, transcript_workflow_request,
};
use pod0_domain::{EpisodeId, StateRevision};

use crate::runtime_state::FacadeState;

pub(super) struct RuntimeTranscriptPlan {
    pub(super) plan: TranscriptWorkflowPlan,
    pub(super) expected_selection_revision: StateRevision,
}

struct RuntimeTranscriptPlanInput {
    input: TranscriptWorkflowPlanInput,
    expected_selection_revision: StateRevision,
}

impl FacadeState {
    pub(super) fn transcript_workflow_plan(
        &self,
        episode_id: EpisodeId,
        origin: TranscriptWorkflowOrigin,
        configuration: TranscriptWorkflowConfiguration,
    ) -> Option<RuntimeTranscriptPlan> {
        let prepared = self.transcript_workflow_plan_input(episode_id, origin, configuration)?;
        Some(RuntimeTranscriptPlan {
            plan: plan_transcript_workflow(prepared.input),
            expected_selection_revision: prepared.expected_selection_revision,
        })
    }

    pub(super) fn legacy_transcript_workflow_request(
        &self,
        episode_id: EpisodeId,
        origin: TranscriptWorkflowOrigin,
        configuration: TranscriptWorkflowConfiguration,
    ) -> Option<(TranscriptWorkflowRequest, StateRevision)> {
        let prepared = self.transcript_workflow_plan_input(episode_id, origin, configuration)?;
        let request = transcript_workflow_request(&prepared.input).ok()??;
        Some((request, prepared.expected_selection_revision))
    }

    fn transcript_workflow_plan_input(
        &self,
        episode_id: EpisodeId,
        origin: TranscriptWorkflowOrigin,
        configuration: TranscriptWorkflowConfiguration,
    ) -> Option<RuntimeTranscriptPlanInput> {
        let episode = self
            .listening
            .episodes
            .iter()
            .find(|episode| episode.episode_id == episode_id)?;
        let source_revision = transcript_source_revision(
            &episode.enclosure_url,
            episode.enclosure_mime_type.as_deref(),
            episode.duration_milliseconds,
        )?;
        let selected = self
            .transcript_store
            .as_ref()?
            .selected_summary(episode_id)
            .ok()?;
        let current = selected
            .as_ref()
            .filter(|value| value.source_revision == source_revision);
        let committed_transcript = current.as_ref().map(|value| CommittedTranscriptGeneration {
            source_revision: value.source_revision.clone(),
            transcript_version_id: value.transcript_version_id,
            content_digest: value.transcript_content_digest,
        });
        let embedding_space_id = self
            .recall_configuration
            .embedding_space_id
            .into_bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let selected_evidence_input_version = current.as_ref().and_then(|transcript| {
            let evidence = self
                .evidence_store
                .as_ref()?
                .selected_generation(episode_id)
                .ok()??;
            (evidence.transcript_version_id == transcript.transcript_version_id)
                .then(|| {
                    transcript_evidence_input_version(
                        transcript.transcript_version_id,
                        transcript.transcript_content_digest,
                        &embedding_space_id,
                    )
                })
                .flatten()
        });
        let publisher = episode.feed_metadata.publisher_transcript.as_ref();
        Some(RuntimeTranscriptPlanInput {
            expected_selection_revision: selected
                .as_ref()
                .map_or(StateRevision::INITIAL, |value| value.selection_revision),
            input: TranscriptWorkflowPlanInput {
                episode_id,
                source_revision,
                committed_transcript,
                selected_evidence_input_version,
                origin,
                configured_provider: configuration.provider,
                configured_model: configuration.model,
                remote_audio_url: episode.enclosure_url.clone(),
                local_audio_url: configuration.local_audio_url,
                publisher_transcript_url: publisher.map(|value| value.url.clone()),
                publisher_mime_hint: publisher.and_then(|value| value.media_type.clone()),
                auto_publisher_enabled: configuration.auto_publisher_enabled,
                auto_provider_enabled: configuration.auto_provider_enabled,
                credential_available: configuration.credential_available,
                embedding_space_id,
            },
        })
    }
}

impl RuntimeTranscriptPlan {
    pub(super) fn is_current(&self) -> bool {
        matches!(self.plan.generation, TranscriptGenerationDecision::Current)
            && matches!(self.plan.evidence, TranscriptEvidenceDecision::Current)
    }
}
