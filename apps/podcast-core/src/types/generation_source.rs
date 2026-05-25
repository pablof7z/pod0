use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum GenerationSource {
    InAppChat {
        conversation_id: Uuid,
    },
    Nostr {
        root_event_id: String,
        peer_pubkey_hex: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn in_app_chat_round_trip() {
        let value = GenerationSource::InAppChat {
            conversation_id: Uuid::nil(),
        };
        let json = serde_json::to_string(&value).unwrap();
        let back: GenerationSource = serde_json::from_str(&json).unwrap();
        assert_eq!(value, back);
    }

    #[test]
    fn nostr_round_trip() {
        let value = GenerationSource::Nostr {
            root_event_id: "deadbeef".into(),
            peer_pubkey_hex: "cafebabe".into(),
        };
        let json = serde_json::to_string(&value).unwrap();
        let back: GenerationSource = serde_json::from_str(&json).unwrap();
        assert_eq!(value, back);
    }
}
