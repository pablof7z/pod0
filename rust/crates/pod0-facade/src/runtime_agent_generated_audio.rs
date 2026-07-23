use pod0_application::{
    AgentActionObservation, AgentActionOutcome, AgentCapabilityOutcome, AgentToolAction,
    AgentTurnState, AgentWorkflowAcceptance, agent_generated_artifact_id,
    agent_generated_audio_evidence_is_valid, agent_generated_episode_id,
    agent_generated_script_digest, default_agent_generated_podcast_id,
};
use pod0_domain::{CommandId, GeneratedAudioArtifactProvenance, UnixTimestampMilliseconds};
use pod0_storage::{
    AgentCommandContext, AgentGeneratedAudioCommitInput, AgentMutationOutcome, AgentStore,
    StorageError,
};
use serde_json::json;

use crate::runtime_agent_modules::identity::{agent_fingerprint, continuation_model_fence_id};

pub(super) fn commit_generated_audio_observation(
    store: &AgentStore,
    mut state: AgentTurnState,
    request_id: pod0_domain::HostRequestId,
    proposal_id: pod0_domain::AgentProposalId,
    execution_fence_id: pod0_domain::AgentExecutionFenceId,
    outcome: &AgentCapabilityOutcome,
    observed_at: UnixTimestampMilliseconds,
) -> Result<AgentTurnState, StorageError> {
    let AgentCapabilityOutcome::GeneratedAudioStaged { evidence } = outcome else {
        return Err(StorageError::AgentTurnConflict);
    };
    let before = state.projection();
    let proposal = before
        .proposal
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
    let target = pod0_application::AgentGeneratedAudioTarget {
        artifact_id: agent_generated_artifact_id(proposal.proposal_id, proposal.proposal_digest),
        maximum_bytes: pod0_application::MAX_AGENT_GENERATED_AUDIO_BYTES,
    };
    if proposal.proposal_id != proposal_id
        || before.execution_fence_id != Some(execution_fence_id)
        || !agent_generated_audio_evidence_is_valid(evidence, target)
    {
        return Err(StorageError::AgentTurnConflict);
    }

    let expected_revision = before.revision;
    if before.stage == pod0_application::AgentTurnStage::Executing {
        let result = json!({
            "generated_episode": true,
            "title": title,
            "media_type": evidence.media_type,
            "byte_count": evidence.byte_count,
        })
        .to_string();
        if state.observe_action(AgentActionObservation {
            proposal_id,
            execution_fence_id,
            outcome: AgentActionOutcome::Succeeded {
                bounded_result: result,
                artifact_id: Some(evidence.artifact_id),
                recall_evidence: Vec::new(),
            },
            observed_at,
        }) != AgentWorkflowAcceptance::Updated
        {
            return Err(StorageError::AgentTurnConflict);
        }
        let projection = state.projection();
        let continuation_fence =
            continuation_model_fence_id(projection.turn_id, projection.revision);
        if state.continue_after_commit(continuation_fence, observed_at)
            != AgentWorkflowAcceptance::Updated
        {
            return Err(StorageError::AgentTurnConflict);
        }
    }

    let projection = state.projection();
    let commit = projection
        .commit
        .as_ref()
        .ok_or(StorageError::InvalidAgentState)?;
    let resolved_podcast_id = podcast_id.unwrap_or_else(default_agent_generated_podcast_id);
    let episode_id = agent_generated_episode_id(resolved_podcast_id, &evidence.file_url);
    let input = AgentGeneratedAudioCommitInput {
        podcast_id: resolved_podcast_id,
        episode_id,
        title: title.clone(),
        audio_url: evidence.file_url.clone(),
        media_type: evidence.media_type.clone(),
        duration_milliseconds: evidence.duration_milliseconds,
        provenance: GeneratedAudioArtifactProvenance {
            artifact_id: evidence.artifact_id,
            conversation_id: projection.conversation_id,
            turn_id: projection.turn_id,
            proposal_id,
            commit_id: commit.commit_id,
            media_content_digest: evidence.content_digest,
            script_content_digest: agent_generated_script_digest(script),
            media_byte_count: evidence.byte_count,
            voice_id: voice_id.clone(),
            model_reference: state.model_reference().to_owned(),
            committed_at: commit.committed_at,
        },
    };
    let fingerprint = agent_fingerprint(
        b"pod0:agent-generated-audio-observation:v1",
        &[
            &projection.turn_id.into_bytes(),
            &evidence.artifact_id.into_bytes(),
        ],
    );
    let outcome = store.commit_generated_audio(
        AgentCommandContext {
            command_id: CommandId::from_bytes(request_id.into_bytes()),
            command_fingerprint: fingerprint,
            observed_at,
        },
        expected_revision,
        &state,
        &input,
    )?;
    Ok(match outcome {
        AgentMutationOutcome::Applied(state) | AgentMutationOutcome::Duplicate(state) => state,
    })
}
