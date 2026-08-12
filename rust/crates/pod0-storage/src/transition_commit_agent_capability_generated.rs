use pod0_application::{
    AgentActionOutcome, AgentCapabilityOutcome, AgentGeneratedAudioTarget, AgentToolAction,
    DurableAgentCapabilityOutcome, MAX_AGENT_GENERATED_AUDIO_BYTES, agent_generated_artifact_id,
    agent_generated_audio_evidence_is_valid, agent_generated_episode_id,
    agent_generated_script_digest, default_agent_generated_podcast_id,
};
use pod0_domain::GeneratedAudioArtifactProvenance;
use serde_json::json;

use crate::{AgentGeneratedAudioCommitInput, StorageError};

pub(super) fn generated_audio_action_outcome(
    state: &pod0_application::AgentTurnState,
    evidence: &pod0_application::AgentGeneratedAudioEvidence,
) -> Result<AgentActionOutcome, StorageError> {
    let projection = state.projection();
    let proposal = projection
        .proposal
        .as_ref()
        .ok_or(StorageError::InvalidAgentState)?;
    if !matches!(proposal.action, AgentToolAction::GenerateTtsEpisode { .. }) {
        return Err(StorageError::AgentTurnConflict);
    }
    let target = AgentGeneratedAudioTarget {
        artifact_id: agent_generated_artifact_id(proposal.proposal_id, proposal.proposal_digest),
        maximum_bytes: MAX_AGENT_GENERATED_AUDIO_BYTES,
    };
    if !agent_generated_audio_evidence_is_valid(evidence, target) {
        return Err(StorageError::AgentTurnConflict);
    }
    Ok(AgentActionOutcome::Succeeded {
        bounded_result: json!({
            "generated_episode": true,
            "media_type": evidence.media_type,
            "byte_count": evidence.byte_count,
        })
        .to_string(),
        artifact_id: Some(evidence.artifact_id),
        recall_evidence: Vec::new(),
    })
}

pub(super) fn generated_audio_input(
    state: &pod0_application::AgentTurnState,
    observation: &pod0_application::DurableAgentCapabilityHostObservation,
) -> Result<Option<AgentGeneratedAudioCommitInput>, StorageError> {
    let DurableAgentCapabilityOutcome::Observed {
        proposal_id,
        outcome: AgentCapabilityOutcome::GeneratedAudioStaged { evidence },
        ..
    } = &observation.outcome
    else {
        return Ok(None);
    };
    let projection = state.projection();
    let proposal = projection
        .proposal
        .as_ref()
        .ok_or(StorageError::InvalidAgentState)?;
    let commit = projection
        .commit
        .as_ref()
        .ok_or(StorageError::InvalidAgentState)?;
    let AgentToolAction::GenerateTtsEpisode {
        podcast_id,
        title,
        script,
        voice_id,
    } = &proposal.action
    else {
        return Err(StorageError::AgentTurnConflict);
    };
    let podcast_id = podcast_id.unwrap_or_else(default_agent_generated_podcast_id);
    Ok(Some(AgentGeneratedAudioCommitInput {
        podcast_id,
        episode_id: agent_generated_episode_id(podcast_id, &evidence.file_url),
        title: title.clone(),
        audio_url: evidence.file_url.clone(),
        media_type: evidence.media_type.clone(),
        duration_milliseconds: evidence.duration_milliseconds,
        provenance: GeneratedAudioArtifactProvenance {
            artifact_id: evidence.artifact_id,
            conversation_id: projection.conversation_id,
            turn_id: projection.turn_id,
            proposal_id: *proposal_id,
            commit_id: commit.commit_id,
            media_content_digest: evidence.content_digest,
            script_content_digest: agent_generated_script_digest(script),
            media_byte_count: evidence.byte_count,
            voice_id: voice_id.clone(),
            model_reference: state.model_reference().to_owned(),
            committed_at: commit.committed_at,
        },
    }))
}
