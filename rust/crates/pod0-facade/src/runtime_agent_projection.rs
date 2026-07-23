use pod0_application::{AgentConversationProjection, CoreFailureCode};
use pod0_domain::ConversationId;

use crate::runtime_state::{FacadeState, failure};

impl FacadeState {
    pub(crate) fn agent_conversation_projection(
        &self,
        conversation_id: ConversationId,
        offset: u32,
        max_items: u16,
    ) -> AgentConversationProjection {
        let Some(store) = &self.agent_store else {
            return AgentConversationProjection {
                conversation_id,
                turns: Vec::new(),
                has_more: false,
                failure: Some(failure(CoreFailureCode::StorageUnavailable)),
            };
        };
        match store.turn_page(conversation_id, offset, max_items) {
            Ok(page) => AgentConversationProjection {
                conversation_id,
                turns: page.items,
                has_more: page.has_more,
                failure: None,
            },
            Err(_) => AgentConversationProjection {
                conversation_id,
                turns: Vec::new(),
                has_more: false,
                failure: Some(failure(CoreFailureCode::StorageUnavailable)),
            },
        }
    }
}
