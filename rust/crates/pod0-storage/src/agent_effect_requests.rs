use pod0_application::{
    AgentApprovalRequest, AgentCapabilityExecutionMode, AgentCapabilityRequest,
    AgentGeneratedAudioTarget, AgentMessageProjection, AgentModelExecutionRequest,
    AgentToolAction, AgentTurnState, DurableAgentApprovalEffectRequest,
    DurableAgentCapabilityEffectRequest, DurableAgentModelEffectRequest,
    MAX_AGENT_GENERATED_AUDIO_BYTES, MAX_AGENT_MODEL_OUTPUT_BYTES,
    MAX_AGENT_PROJECTION_MESSAGES, agent_generated_artifact_id, agent_tool_definitions,
};
use pod0_domain::{CommandId, HostRequestId, StateRevision, UnixTimestampMilliseconds};
use rusqlite::{Connection, params};

use crate::{StorageError, agent_store::read_turn};

pub(crate) fn model_effect_request(
    connection: &Connection,
    state: &AgentTurnState,
    command_id: CommandId,
    deadline_at: Option<UnixTimestampMilliseconds>,
) -> Result<DurableAgentModelEffectRequest, StorageError> {
    let projection = state.projection();
    let model_fence_id = projection
        .execution_fence_id
        .ok_or(StorageError::InvalidAgentState)?;
    let tools = if projection.commit.is_some() {
        Vec::new()
    } else {
        agent_tool_definitions(state.available_tools()).ok_or(StorageError::InvalidAgentState)?
    };
    Ok(DurableAgentModelEffectRequest {
        request_id: pod0_application::agent_model_request_id(projection.turn_id, model_fence_id),
        command_id,
        cancellation_id: state.cancellation_id(),
        issued_revision: projection.revision,
        deadline_at,
        execution: AgentModelExecutionRequest {
            conversation_id: projection.conversation_id,
            turn_id: projection.turn_id,
            model_fence_id,
            model_reference: state.model_reference().to_owned(),
            messages: conversation_messages(connection, state)?,
            tool_definitions: tools,
            maximum_output_bytes: MAX_AGENT_MODEL_OUTPUT_BYTES,
        },
    })
}

pub(crate) fn approval_effect_request(
    state: &AgentTurnState,
    command_id: CommandId,
    deadline_at: Option<UnixTimestampMilliseconds>,
) -> Result<DurableAgentApprovalEffectRequest, StorageError> {
    let projection = state.projection();
    let proposal = projection
        .proposal
        .ok_or(StorageError::InvalidAgentState)?;
    Ok(DurableAgentApprovalEffectRequest {
        request_id: pod0_application::agent_approval_request_id(
            projection.turn_id,
            proposal.proposal_id,
            proposal.proposal_digest,
        ),
        command_id,
        cancellation_id: state.cancellation_id(),
        issued_revision: projection.revision,
        deadline_at,
        approval: AgentApprovalRequest {
            turn_id: projection.turn_id,
            proposal,
        },
    })
}

pub(crate) fn capability_effect_request(
    state: &AgentTurnState,
    command_id: CommandId,
    deadline_at: Option<UnixTimestampMilliseconds>,
) -> Result<DurableAgentCapabilityEffectRequest, StorageError> {
    let projection = state.projection();
    let proposal = projection
        .proposal
        .ok_or(StorageError::InvalidAgentState)?;
    let execution_fence_id = projection
        .execution_fence_id
        .ok_or(StorageError::InvalidAgentState)?;
    let generated_audio_target = matches!(proposal.action, AgentToolAction::GenerateTtsEpisode { .. })
        .then(|| AgentGeneratedAudioTarget {
            artifact_id: agent_generated_artifact_id(
                proposal.proposal_id,
                proposal.proposal_digest,
            ),
            maximum_bytes: MAX_AGENT_GENERATED_AUDIO_BYTES,
        });
    Ok(DurableAgentCapabilityEffectRequest {
        request_id: pod0_application::agent_capability_request_id(
            projection.turn_id,
            proposal.proposal_id,
            execution_fence_id,
        ),
        command_id,
        cancellation_id: state.cancellation_id(),
        issued_revision: projection.revision,
        deadline_at,
        capability: AgentCapabilityRequest {
            turn_id: projection.turn_id,
            proposal_id: proposal.proposal_id,
            proposal_digest: proposal.proposal_digest,
            execution_fence_id,
            execution_mode: AgentCapabilityExecutionMode::Perform,
            generated_audio_target,
            action: proposal.action,
        },
    })
}

fn conversation_messages(
    connection: &Connection,
    state: &AgentTurnState,
) -> Result<Vec<AgentMessageProjection>, StorageError> {
    let current = state.projection();
    let mut statement = connection
        .prepare(
            "SELECT turn_id FROM pod0_agent_turns WHERE conversation_id=?1 \
             ORDER BY created_at_ms DESC,rowid DESC LIMIT ?2",
        )
        .map_err(|error| StorageError::sqlite("prepare exact agent context", error))?;
    let rows = statement
        .query_map(
            params![
                current.conversation_id.into_bytes().as_slice(),
                i64::try_from(MAX_AGENT_PROJECTION_MESSAGES)
                    .map_err(|_| StorageError::InvalidAgentState)?
            ],
            |row| row.get::<_, Vec<u8>>(0),
        )
        .map_err(|error| StorageError::sqlite("read exact agent context", error))?;
    let mut turns = Vec::new();
    let mut found_current = false;
    for row in rows {
        let bytes: [u8; 16] = row
            .map_err(|error| StorageError::sqlite("decode exact agent context", error))?
            .try_into()
            .map_err(|_| StorageError::InvalidAgentState)?;
        let turn_id = pod0_domain::AgentTurnId::from_bytes(bytes);
        if turn_id == current.turn_id {
            turns.push(current.clone());
            found_current = true;
        } else {
            turns.push(read_turn(connection, turn_id)?.ok_or(StorageError::AgentTurnNotFound)?.projection());
        }
    }
    if !found_current {
        turns.insert(0, current);
    }
    turns.reverse();
    let mut messages = turns
        .into_iter()
        .flat_map(|turn| turn.messages)
        .collect::<Vec<_>>();
    if messages.len() > MAX_AGENT_PROJECTION_MESSAGES {
        messages.drain(..messages.len() - MAX_AGENT_PROJECTION_MESSAGES);
    }
    Ok(messages)
}

#[allow(dead_code)]
fn _identity_types(_: HostRequestId, _: StateRevision) {}
