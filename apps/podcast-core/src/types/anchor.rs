use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Anchor {
    Note {
        id: Uuid,
    },
    Friend {
        id: Uuid,
    },
    Episode {
        id: Uuid,
        #[serde(default)]
        position_seconds: f64,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anchor_note_round_trip() {
        let value = Anchor::Note { id: Uuid::nil() };
        let json = serde_json::to_string(&value).unwrap();
        let back: Anchor = serde_json::from_str(&json).unwrap();
        assert_eq!(value, back);
    }

    #[test]
    fn anchor_episode_round_trip() {
        let value = Anchor::Episode {
            id: Uuid::nil(),
            position_seconds: 42.5,
        };
        let json = serde_json::to_string(&value).unwrap();
        let back: Anchor = serde_json::from_str(&json).unwrap();
        assert_eq!(value, back);
    }
}
