use pod0_application::CommandEnvelope;

pub(super) fn observation_action_envelope(
    reaction: &CommandEnvelope,
    label: &str,
) -> CommandEnvelope {
    use sha2::{Digest as _, Sha256};
    let mut hash = Sha256::new();
    hash.update(b"pod0-playback-observation-action-v1\0");
    hash.update(reaction.command_id.into_bytes());
    hash.update(label.as_bytes());
    let digest: [u8; 32] = hash.finalize().into();
    CommandEnvelope {
        command_id: pod0_domain::CommandId::from_bytes(
            digest[..16].try_into().expect("fixed digest prefix"),
        ),
        cancellation_id: reaction.cancellation_id,
        expected_revision: None,
        command: pod0_application::ApplicationCommand::Playback {
            command: pod0_application::PlaybackCommand::Restore,
        },
    }
}
