use pod0_application::{
    CommandEnvelope, CoreFailureCode, EvidenceChunkPolicy, TranscriptEvidenceInput,
    TranscriptSegmentInput, transcript_evidence_input_version,
};
use pod0_domain::{EpisodeId, TranscriptArtifact};
use pod0_storage::{
    StoredTranscriptWorkflowStage, TranscriptWorkflowCommitInput, TranscriptWorkflowRecord,
};

use crate::runtime_evidence_state::EvidenceIndexCompletion;
use crate::runtime_state::FacadeState;

impl FacadeState {
    pub(super) fn finalize_transcript_completion(
        &mut self,
        record: &TranscriptWorkflowRecord,
    ) -> bool {
        let (Some(request_id), Some(artifact_id), Some(store), Some(transcript_store)) = (
            record.request_id,
            record.completion_artifact_id,
            self.store.clone(),
            self.transcript_store.clone(),
        ) else {
            return false;
        };
        let Ok(Some(artifact)) = transcript_store.artifact(artifact_id) else {
            return false;
        };
        let embedding_space_id = embedding_space_id(self);
        let Some(input_version) = transcript_evidence_input_version(
            artifact.transcript_version_id,
            artifact.content_digest,
            &embedding_space_id,
        ) else {
            return false;
        };
        let Ok(receipt) = store.commit_transcript_workflow(TranscriptWorkflowCommitInput {
            episode_id: record.episode_id,
            request_id,
            evidence_input_version: input_version,
            completed_at_ms: self.now().value,
        }) else {
            return false;
        };
        let _ = self.reload_listening();
        self.retire_transcript_request(request_id);
        self.resume_transcript_evidence(&receipt.workflow)
    }

    pub(super) fn resume_transcript_evidence(&mut self, record: &TranscriptWorkflowRecord) -> bool {
        if record.stage != StoredTranscriptWorkflowStage::EvidenceRequested {
            return false;
        }
        let (Some(input_version), Some(transcript_store)) = (
            record.evidence_input_version.clone(),
            self.transcript_store.as_ref(),
        ) else {
            return false;
        };
        let Ok(Some(artifact)) = transcript_store.selected_artifact(record.episode_id) else {
            return false;
        };
        let envelope = CommandEnvelope {
            command_id: record.command_id,
            cancellation_id: record.cancellation_id,
            expected_revision: None,
            command: pod0_application::ApplicationCommand::Unsupported { wire_code: 0 },
        };
        self.start_evidence_index(
            &envelope,
            evidence_input(&artifact),
            EvidenceChunkPolicy::default(),
            EvidenceIndexCompletion::TranscriptWorkflow {
                workflow_id: record.request.workflow_id,
                input_version,
            },
        );
        true
    }

    pub(super) fn start_current_transcript_evidence(
        &mut self,
        envelope: &CommandEnvelope,
        episode_id: EpisodeId,
    ) {
        let artifact = self
            .transcript_store
            .as_ref()
            .and_then(|store| store.selected_artifact(episode_id).ok())
            .flatten();
        let Some(artifact) = artifact else {
            self.fail(envelope.command_id, CoreFailureCode::NotFound);
            return;
        };
        self.start_evidence_index(
            envelope,
            evidence_input(&artifact),
            EvidenceChunkPolicy::default(),
            EvidenceIndexCompletion::EvidenceRebuild,
        );
    }
}

fn evidence_input(artifact: &TranscriptArtifact) -> TranscriptEvidenceInput {
    TranscriptEvidenceInput {
        episode_id: artifact.episode_id,
        podcast_id: artifact.podcast_id,
        source_revision: artifact.source_revision.clone(),
        source: artifact.provenance.source,
        provider: artifact.provenance.provider.clone(),
        source_payload_digest: artifact.provenance.source_payload_digest,
        segments: artifact
            .segments
            .iter()
            .map(|segment| TranscriptSegmentInput {
                text: segment.text.clone(),
                start_milliseconds: segment.start_milliseconds,
                end_milliseconds: segment.end_milliseconds,
                speaker_id: segment.speaker_id,
            })
            .collect(),
    }
}

fn embedding_space_id(state: &FacadeState) -> String {
    state
        .recall_configuration
        .embedding_space_id
        .into_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
