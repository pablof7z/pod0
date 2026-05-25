use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TriageDecision {
    Inbox,
    Archived,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn triage_round_trip() {
        let value = TriageDecision::Inbox;
        let json = serde_json::to_string(&value).unwrap();
        let back: TriageDecision = serde_json::from_str(&json).unwrap();
        assert_eq!(value, back);
    }
}
