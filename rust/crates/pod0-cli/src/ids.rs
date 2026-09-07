use sha2::{Digest as _, Sha256};

use pod0_facade::{CancellationId, CommandId, ConversationId, EpisodeId, PodcastId};

pub(crate) struct IdFactory {
    seed: [u8; 32],
    sequence: u64,
}

impl IdFactory {
    pub(crate) fn new() -> Self {
        let mut hash = Sha256::new();
        hash.update(b"pod0-cli:id-seed:v1\0");
        hash.update(std::process::id().to_be_bytes());
        hash.update(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |duration| duration.as_nanos())
                .to_be_bytes(),
        );
        Self {
            seed: hash.finalize().into(),
            sequence: 0,
        }
    }

    pub(crate) fn command(&mut self) -> (CommandId, CancellationId) {
        self.sequence = self.sequence.saturating_add(1);
        (
            CommandId::from_bytes(self.derive(b"command")),
            CancellationId::from_bytes(self.derive(b"cancellation")),
        )
    }

    fn derive(&self, label: &[u8]) -> [u8; 16] {
        let mut hash = Sha256::new();
        hash.update(b"pod0-cli:id:v1\0");
        hash.update(self.seed);
        hash.update(self.sequence.to_be_bytes());
        hash.update(label);
        hash.finalize()[..16].try_into().expect("digest prefix")
    }
}

pub(crate) fn encode_id(bytes: [u8; 16]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub(crate) fn parse_podcast_id(value: &str) -> Option<PodcastId> {
    parse_id(value).map(PodcastId::from_bytes)
}

pub(crate) fn parse_episode_id(value: &str) -> Option<EpisodeId> {
    parse_id(value).map(EpisodeId::from_bytes)
}

pub(crate) fn parse_conversation_id(value: &str) -> Option<ConversationId> {
    parse_id(value).map(ConversationId::from_bytes)
}

fn parse_id(value: &str) -> Option<[u8; 16]> {
    if value.len() != 32 {
        return None;
    }
    let mut bytes = [0_u8; 16];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16).ok()?;
    }
    Some(bytes)
}
