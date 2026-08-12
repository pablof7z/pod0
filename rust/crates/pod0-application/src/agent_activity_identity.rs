use pod0_domain::{
    AgentAuthorizationId, AgentExecutionFenceId, AgentProposalId, AgentTurnId, ContentDigest,
    HostRequestId, StateRevision,
};
use sha2::{Digest as _, Sha256};

#[must_use]
pub fn agent_model_request_id(
    turn_id: AgentTurnId,
    fence_id: AgentExecutionFenceId,
) -> HostRequestId {
    request_id(
        b"pod0:agent-model-request:v1",
        &[&turn_id.into_bytes(), &fence_id.into_bytes()],
    )
}

#[must_use]
pub fn agent_approval_request_id(
    turn_id: AgentTurnId,
    proposal_id: AgentProposalId,
    digest: ContentDigest,
) -> HostRequestId {
    request_id(
        b"pod0:agent-approval-request:v1",
        &[
            &turn_id.into_bytes(),
            &proposal_id.into_bytes(),
            &digest.into_bytes(),
        ],
    )
}

#[must_use]
pub fn agent_authorization_id(request_id: HostRequestId) -> AgentAuthorizationId {
    AgentAuthorizationId::from_bytes(derived(
        b"pod0:agent-authorization:v1",
        &[&request_id.into_bytes()],
    ))
}

#[must_use]
pub fn agent_execution_fence_id(
    proposal_id: AgentProposalId,
    digest: ContentDigest,
) -> AgentExecutionFenceId {
    AgentExecutionFenceId::from_bytes(derived(
        b"pod0:agent-execution-fence:v1",
        &[&proposal_id.into_bytes(), &digest.into_bytes()],
    ))
}

#[must_use]
pub fn agent_capability_request_id(
    turn_id: AgentTurnId,
    proposal_id: AgentProposalId,
    fence_id: AgentExecutionFenceId,
) -> HostRequestId {
    request_id(
        b"pod0:agent-capability-request:v1",
        &[
            &turn_id.into_bytes(),
            &proposal_id.into_bytes(),
            &fence_id.into_bytes(),
        ],
    )
}

#[must_use]
pub fn continuation_model_fence_id(
    turn_id: AgentTurnId,
    revision: StateRevision,
) -> AgentExecutionFenceId {
    AgentExecutionFenceId::from_bytes(derived(
        b"pod0:agent-continuation-model-fence:v1",
        &[&turn_id.into_bytes(), &revision.value.to_be_bytes()],
    ))
}

fn request_id(domain: &[u8], parts: &[&[u8]]) -> HostRequestId {
    HostRequestId::from_bytes(derived(domain, parts))
}

fn derived(domain: &[u8], parts: &[&[u8]]) -> [u8; 16] {
    let mut hash = Sha256::new();
    hash.update(domain);
    hash.update([0]);
    for part in parts {
        hash.update((part.len() as u64).to_be_bytes());
        hash.update(part);
    }
    let digest: [u8; 32] = hash.finalize().into();
    digest[..16].try_into().expect("fixed digest prefix")
}
