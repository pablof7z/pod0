use pod0_application::ApplicationCommand;
use sha2::{Digest, Sha256};

pub(super) fn hash_agent_command(hash: &mut Sha256, command: &ApplicationCommand) {
    match command {
        ApplicationCommand::StartAgentTurn {
            conversation_id,
            user_input,
            model_reference,
        } => {
            hash.update(b"start-agent-turn\0");
            if let Some(conversation_id) = conversation_id {
                hash.update([1]);
                hash.update(conversation_id.into_bytes());
            } else {
                hash.update([0]);
            }
            hash.update(user_input.as_bytes());
            hash.update([0]);
            hash.update(model_reference.as_bytes());
        }
        ApplicationCommand::CancelAgentTurn {
            turn_id,
            expected_turn_revision,
        } => {
            hash.update(b"cancel-agent-turn\0");
            hash.update(turn_id.into_bytes());
            hash.update(expected_turn_revision.value.to_be_bytes());
        }
        _ => unreachable!("agent command fingerprint called for non-agent command"),
    }
}
