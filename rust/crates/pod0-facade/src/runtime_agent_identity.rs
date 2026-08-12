use pod0_domain::{
    AgentAuthorizationId, AgentExecutionFenceId, AgentProposalId, AgentTurnId, CommandId,
    ContentDigest, HostRequestId, StateRevision,
};
use sha2::{Digest, Sha256};

pub(super) fn agent_turn_id(command_id: CommandId) -> AgentTurnId {
    AgentTurnId::from_bytes(command_id.into_bytes())
}

pub(super) fn agent_command_id(domain: &[u8], turn_id: AgentTurnId) -> CommandId {
    CommandId::from_bytes(derived(domain, &[&turn_id.into_bytes()]))
}

pub(super) fn agent_execution_fence_id(
    proposal_id: AgentProposalId,
    digest: ContentDigest,
) -> AgentExecutionFenceId {
    pod0_application::agent_execution_fence_id(proposal_id, digest)
}

pub(super) fn agent_authorization_id(request_id: HostRequestId) -> AgentAuthorizationId {
    pod0_application::agent_authorization_id(request_id)
}

pub(super) fn model_fence_id(turn_id: AgentTurnId) -> AgentExecutionFenceId {
    AgentExecutionFenceId::from_bytes(derived(
        b"pod0:agent-model-fence:v1",
        &[&turn_id.into_bytes()],
    ))
}

pub(super) fn continuation_model_fence_id(
    turn_id: AgentTurnId,
    revision: StateRevision,
) -> AgentExecutionFenceId {
    pod0_application::continuation_model_fence_id(turn_id, revision)
}

pub(crate) fn model_request_id(
    turn_id: AgentTurnId,
    fence_id: AgentExecutionFenceId,
) -> HostRequestId {
    pod0_application::agent_model_request_id(turn_id, fence_id)
}

pub(super) fn approval_request_id(
    turn_id: AgentTurnId,
    proposal_id: AgentProposalId,
    digest: ContentDigest,
) -> HostRequestId {
    pod0_application::agent_approval_request_id(turn_id, proposal_id, digest)
}

pub(super) fn capability_request_id(
    turn_id: AgentTurnId,
    proposal_id: AgentProposalId,
    fence_id: AgentExecutionFenceId,
) -> HostRequestId {
    pod0_application::agent_capability_request_id(turn_id, proposal_id, fence_id)
}

pub(super) fn agent_fingerprint(domain: &[u8], parts: &[&[u8]]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    hasher.update([0]);
    for part in parts {
        hasher.update((part.len() as u64).to_be_bytes());
        hasher.update(part);
    }
    hasher.finalize().into()
}

fn derived(domain: &[u8], parts: &[&[u8]]) -> [u8; 16] {
    let digest = agent_fingerprint(domain, parts);
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes
}
