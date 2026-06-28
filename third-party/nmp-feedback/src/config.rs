#[cfg(feature = "nmp")]
use nmp_planner::interest::{InterestId, InterestLifecycle, InterestScope, LogicalInterest};
#[cfg(feature = "nmp")]
use nmp_planner::stable_hash::stable_hash64;
#[cfg(feature = "nmp")]
use nmp_core::substrate::ViewDependencies;

pub const DEFAULT_FEEDBACK_RELAY: &str = "wss://relay.tenex.chat";
pub const KIND_FEEDBACK_NOTE: u32 = 1;
pub const KIND_FEEDBACK_THREAD_METADATA: u32 = 513;

const DEFAULT_MAX_EVENTS: usize = 500;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeedbackConfig {
    pub project_coordinate: String,
    pub relay_url: String,
    pub interest_namespace: String,
    pub max_events: usize,
    pub protected_marker: bool,
    pub agent_pubkey: Option<String>,
}

impl FeedbackConfig {
    #[must_use]
    pub fn new(project_coordinate: impl Into<String>) -> Self {
        let project_coordinate = project_coordinate.into();
        Self {
            interest_namespace: format!("nmp.feedback.{project_coordinate}"),
            project_coordinate,
            relay_url: DEFAULT_FEEDBACK_RELAY.to_string(),
            max_events: DEFAULT_MAX_EVENTS,
            protected_marker: true,
            agent_pubkey: None,
        }
    }

    #[must_use]
    pub fn with_relay(mut self, relay_url: impl Into<String>) -> Self {
        self.relay_url = relay_url.into();
        self
    }

    #[must_use]
    pub fn with_interest_namespace(mut self, namespace: impl Into<String>) -> Self {
        self.interest_namespace = namespace.into();
        self
    }

    #[must_use]
    pub fn with_max_events(mut self, max_events: usize) -> Self {
        self.max_events = max_events.max(1);
        self
    }

    #[must_use]
    pub fn with_agent_pubkey(mut self, pubkey: impl Into<String>) -> Self {
        self.agent_pubkey = Some(pubkey.into());
        self
    }

    #[must_use]
    pub fn without_protected_marker(mut self) -> Self {
        self.protected_marker = false;
        self
    }

    #[must_use]
    pub fn relay_seed(&self) -> (String, String) {
        (self.relay_url.clone(), "read".to_string())
    }

    #[cfg(feature = "nmp")]
    #[must_use]
    pub fn interest(&self) -> LogicalInterest {
        let mut interest = ViewDependencies {
            kinds: vec![KIND_FEEDBACK_NOTE, KIND_FEEDBACK_THREAD_METADATA],
            tag_refs: vec![("a".to_string(), self.project_coordinate.clone())],
            relay_pin: Some(self.relay_url.clone()),
            limit: Some(self.max_events as u32),
            ..Default::default()
        }
        .into_logical_interest(
            InterestId(stable_hash64(&self.interest_namespace)),
            InterestScope::Global,
            InterestLifecycle::OneShot,
        );
        interest.shape.relay_pin = Some(self.relay_url.clone());
        interest
    }

    #[must_use]
    pub fn tags(
        &self,
        category: &str,
        parent_event_id: Option<&str>,
        reply_to_pubkey: Option<&str>,
    ) -> Vec<Vec<String>> {
        let mut tags = vec![
            vec!["a".to_string(), self.project_coordinate.clone()],
            vec!["t".to_string(), category.to_string()],
        ];
        if self.protected_marker {
            tags.push(vec!["-".to_string()]);
        }
        if let Some(agent) = self.agent_pubkey.as_deref().filter(|s| !s.is_empty()) {
            tags.push(vec!["p".to_string(), agent.to_string()]);
        }
        if let Some(parent) = parent_event_id.filter(|s| !s.is_empty()) {
            tags.push(vec![
                "e".to_string(),
                parent.to_string(),
                String::new(),
                "root".to_string(),
            ]);
        }
        if let Some(pk) = reply_to_pubkey.filter(|s| !s.is_empty()) {
            if self.agent_pubkey.as_deref() != Some(pk) {
                tags.push(vec!["p".to_string(), pk.to_string()]);
            }
        }
        tags
    }

    #[cfg(feature = "nmp")]
    #[must_use]
    pub(crate) fn accepts_event(&self, kind: u32, tags: &[Vec<String>]) -> bool {
        if kind != KIND_FEEDBACK_NOTE && kind != KIND_FEEDBACK_THREAD_METADATA {
            return false;
        }
        tags.iter().any(|tag| {
            tag.first().is_some_and(|name| name == "a")
                && tag
                    .get(1)
                    .is_some_and(|coord| coord == &self.project_coordinate)
        })
    }
}

#[cfg(feature = "nmp")]
pub(crate) fn text_note_kind() -> u32 {
    KIND_FEEDBACK_NOTE
}

#[cfg(test)]
mod tests {
    use super::*;

    const COORD: &str = "31933:abc:app";

    #[test]
    fn root_tags_carry_project_category_and_protected_marker() {
        let tags = FeedbackConfig::new(COORD).tags("bug", None, None);
        assert!(tags.contains(&vec!["a".to_string(), COORD.to_string()]));
        assert!(tags.contains(&vec!["t".to_string(), "bug".to_string()]));
        assert!(tags.contains(&vec!["-".to_string()]));
        assert!(!tags.iter().any(|tag| tag.first().is_some_and(|t| t == "e")));
    }

    #[test]
    fn reply_tags_carry_root_marker_and_recipient() {
        let tags = FeedbackConfig::new(COORD).tags("question", Some("root-id"), Some("pubkey"));
        assert!(tags.contains(&vec![
            "e".to_string(),
            "root-id".to_string(),
            String::new(),
            "root".to_string()
        ]));
        assert!(tags.contains(&vec!["p".to_string(), "pubkey".to_string()]));
    }
}
