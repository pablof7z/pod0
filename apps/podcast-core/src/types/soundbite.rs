use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SoundBite {
    pub id: Uuid,
    pub start_secs: f64,
    pub duration_secs: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

impl SoundBite {
    pub fn new(start_secs: f64, duration_secs: f64) -> Self {
        Self {
            id: Uuid::new_v4(),
            start_secs,
            duration_secs,
            title: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn soundbite_round_trip() {
        let mut value = SoundBite::new(10.0, 30.0);
        value.title = Some("Highlight".into());
        let json = serde_json::to_string(&value).unwrap();
        let back: SoundBite = serde_json::from_str(&json).unwrap();
        assert_eq!(value, back);
    }
}
