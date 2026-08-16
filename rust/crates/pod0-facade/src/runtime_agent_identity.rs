use pod0_domain::{AgentExecutionFenceId, AgentTurnId, CommandId};
use sha2::{Digest, Sha256};

pub(super) fn agent_turn_id(command_id: CommandId) -> AgentTurnId {
    AgentTurnId::from_bytes(command_id.into_bytes())
}

pub(super) fn model_fence_id(turn_id: AgentTurnId) -> AgentExecutionFenceId {
    AgentExecutionFenceId::from_bytes(derived(
        b"pod0:agent-model-fence:v1",
        &[&turn_id.into_bytes()],
    ))
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
