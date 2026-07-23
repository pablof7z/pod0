use pod0_application::AgentTurnState;
use pod0_domain::{ContentDigest, ConversationId, UnixTimestampMilliseconds};
use sha2::{Digest as _, Sha256};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentHistoryCutoverState {
    NotStarted,
    Staged { source_generation: u64 },
    Verified { source_generation: u64 },
    Authoritative { source_generation: u64 },
}

impl AgentHistoryCutoverState {
    pub const fn source_generation(self) -> Option<u64> {
        match self {
            Self::NotStarted => None,
            Self::Staged { source_generation }
            | Self::Verified { source_generation }
            | Self::Authoritative { source_generation } => Some(source_generation),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LegacyAgentHistoryTurn {
    pub created_at: UnixTimestampMilliseconds,
    pub state: AgentTurnState,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LegacyAgentHistoryConversation {
    pub conversation_id: ConversationId,
    pub title: String,
    pub created_at: UnixTimestampMilliseconds,
    pub updated_at: UnixTimestampMilliseconds,
    pub turns: Vec<LegacyAgentHistoryTurn>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LegacyAgentHistoryCutoverInput {
    pub backup_digest: ContentDigest,
    pub backup_byte_count: u64,
    pub conversations: Vec<LegacyAgentHistoryConversation>,
    pub observed_at: UnixTimestampMilliseconds,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LegacyAgentHistoryCutoverReport {
    pub state: AgentHistoryCutoverState,
    pub source_fingerprint: Option<ContentDigest>,
    pub backup_digest: Option<ContentDigest>,
    pub backup_byte_count: Option<u64>,
    pub conversation_count: u32,
    pub turn_count: u32,
    pub message_count: u32,
}

#[must_use]
pub fn agent_history_source_fingerprint(input: &LegacyAgentHistoryCutoverInput) -> ContentDigest {
    let mut hash = StableHash::new();
    hash.bytes(&input.backup_digest.into_bytes());
    hash.u64(input.backup_byte_count);
    let mut conversations: Vec<_> = input.conversations.iter().collect();
    conversations.sort_by_key(|value| value.conversation_id.into_bytes());
    hash.u64(conversations.len() as u64);
    for conversation in conversations {
        hash.bytes(&conversation.conversation_id.into_bytes());
        hash.text(&conversation.title);
        hash.i64(conversation.created_at.value());
        hash.i64(conversation.updated_at.value());
        let mut turns: Vec<_> = conversation.turns.iter().collect();
        turns.sort_by_key(|turn| turn.state.projection().turn_id.into_bytes());
        hash.u64(turns.len() as u64);
        for turn in turns {
            hash.i64(turn.created_at.value());
            let bytes = serde_json::to_vec(&turn.state).unwrap_or_default();
            hash.bytes(&bytes);
        }
    }
    ContentDigest::from_bytes(hash.finish())
}

#[must_use]
pub fn agent_history_source_generation(fingerprint: ContentDigest) -> u64 {
    let bytes = fingerprint.into_bytes();
    u64::from_be_bytes(bytes[..8].try_into().expect("digest prefix")) & i64::MAX as u64 | 1
}

pub fn agent_history_counts(
    conversations: &[LegacyAgentHistoryConversation],
) -> (usize, usize, usize) {
    let turns = conversations
        .iter()
        .map(|conversation| conversation.turns.len())
        .sum();
    let messages = conversations
        .iter()
        .flat_map(|conversation| &conversation.turns)
        .map(|turn| turn.state.projection().messages.len())
        .sum();
    (conversations.len(), turns, messages)
}

struct StableHash(Sha256);

impl StableHash {
    fn new() -> Self {
        let mut value = Self(Sha256::new());
        value.bytes(b"pod0-legacy-agent-history-cutover-v1");
        value
    }

    fn bytes(&mut self, value: &[u8]) {
        self.0.update((value.len() as u64).to_be_bytes());
        self.0.update(value);
    }

    fn text(&mut self, value: &str) {
        self.bytes(value.as_bytes());
    }

    fn u64(&mut self, value: u64) {
        self.bytes(&value.to_be_bytes());
    }

    fn i64(&mut self, value: i64) {
        self.bytes(&value.to_be_bytes());
    }

    fn finish(self) -> [u8; 32] {
        self.0.finalize().into()
    }
}
