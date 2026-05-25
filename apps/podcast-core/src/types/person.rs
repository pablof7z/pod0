use serde::{Deserialize, Serialize};
use url::Url;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Person {
    pub id: Uuid,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_url: Option<Url>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link_url: Option<Url>,
}

impl Person {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            role: None,
            group: None,
            image_url: None,
            link_url: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn person_round_trip() {
        let value = Person::new("Alice");
        let json = serde_json::to_string(&value).unwrap();
        let back: Person = serde_json::from_str(&json).unwrap();
        assert_eq!(value, back);
    }
}
