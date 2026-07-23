use pod0_domain::{ConversationId, UnixTimestampMilliseconds};

use crate::{AgentTurnStage, CoreFailure};

pub const MAX_AGENT_CONVERSATION_SUMMARIES: u16 = 100;
pub const MAX_AGENT_CONVERSATION_TITLE_BYTES: usize = 256;
pub const MAX_AGENT_CONVERSATION_PREVIEW_BYTES: usize = 512;

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct AgentConversationSummaryProjection {
    pub conversation_id: ConversationId,
    pub title: String,
    pub preview: String,
    pub turn_count: u32,
    pub latest_stage: AgentTurnStage,
    pub created_at: UnixTimestampMilliseconds,
    pub updated_at: UnixTimestampMilliseconds,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct AgentConversationsProjection {
    pub conversations: Vec<AgentConversationSummaryProjection>,
    pub has_more: bool,
    pub failure: Option<CoreFailure>,
}

#[must_use]
pub fn bounded_agent_summary_text(value: &str, maximum_bytes: usize) -> String {
    if value.len() <= maximum_bytes {
        return value.to_owned();
    }
    let mut end = maximum_bytes;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_text_bounds_preserve_utf8() {
        assert_eq!(bounded_agent_summary_text("hello", 5), "hello");
        assert_eq!(bounded_agent_summary_text("ééé", 5), "éé");
    }
}
