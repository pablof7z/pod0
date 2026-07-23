use pod0_domain::UnixTimestampMilliseconds;

pub const MAX_AGENT_MODEL_USAGE_ENTRIES: usize = 4;
pub const MAX_AGENT_TOKEN_COUNT: u64 = 1_000_000_000;

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, uniffi::Record,
)]
pub struct AgentModelUsageObservation {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub cached_prompt_tokens: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, uniffi::Record)]
pub struct AgentModelUsageProjection {
    pub model_reference: String,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub cached_prompt_tokens: Option<u64>,
    pub observed_at: UnixTimestampMilliseconds,
}

#[must_use]
pub fn agent_model_usage_is_valid(usage: AgentModelUsageObservation) -> bool {
    usage.prompt_tokens <= MAX_AGENT_TOKEN_COUNT
        && usage.completion_tokens <= MAX_AGENT_TOKEN_COUNT
        && usage
            .cached_prompt_tokens
            .is_none_or(|cached| cached <= usage.prompt_tokens && cached <= MAX_AGENT_TOKEN_COUNT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_usage_rejects_impossible_or_unbounded_counts() {
        assert!(agent_model_usage_is_valid(AgentModelUsageObservation {
            prompt_tokens: 100,
            completion_tokens: 20,
            cached_prompt_tokens: Some(40),
        }));
        assert!(!agent_model_usage_is_valid(AgentModelUsageObservation {
            prompt_tokens: 100,
            completion_tokens: 20,
            cached_prompt_tokens: Some(101),
        }));
        assert!(!agent_model_usage_is_valid(AgentModelUsageObservation {
            prompt_tokens: MAX_AGENT_TOKEN_COUNT + 1,
            completion_tokens: 20,
            cached_prompt_tokens: None,
        }));
    }
}
