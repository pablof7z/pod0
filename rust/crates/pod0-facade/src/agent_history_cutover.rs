use pod0_application::LegacyAgentHistoryConversationInput;
use pod0_domain::ContentDigest;
use pod0_storage::{StorageError, agent_history_counts, inspect_legacy_agent_history_cutover};

use crate::agent_history_cutover_mapping::cutover_input;
use crate::{LegacyAgentHistoryCutoverProjection, Pod0Facade};

#[uniffi::export]
impl Pod0Facade {
    pub fn agent_history_cutover(&self) -> LegacyAgentHistoryCutoverProjection {
        let state = self.state();
        let Some(store) = state.store.as_ref() else {
            return LegacyAgentHistoryCutoverProjection::blocked(StorageError::AgentTurnConflict);
        };
        store
            .agent_history_cutover_report()
            .map(LegacyAgentHistoryCutoverProjection::from_report)
            .unwrap_or_else(LegacyAgentHistoryCutoverProjection::blocked)
    }

    pub fn inspect_legacy_agent_history_cutover(
        &self,
        backup_digest: ContentDigest,
        backup_byte_count: u64,
        conversations: Vec<LegacyAgentHistoryConversationInput>,
    ) -> LegacyAgentHistoryCutoverProjection {
        let state = self.state();
        let input =
            match cutover_input(backup_digest, backup_byte_count, conversations, state.now()) {
                Ok(input) => input,
                Err(error) => return LegacyAgentHistoryCutoverProjection::blocked(error),
            };
        match inspect_legacy_agent_history_cutover(&input) {
            Ok((fingerprint, generation)) => {
                let (conversations, turns, messages) = agent_history_counts(&input.conversations);
                LegacyAgentHistoryCutoverProjection::inspected(
                    generation,
                    fingerprint,
                    input.backup_digest,
                    input.backup_byte_count,
                    conversations,
                    turns,
                    messages,
                )
            }
            Err(error) => LegacyAgentHistoryCutoverProjection::blocked(error),
        }
    }

    pub fn stage_legacy_agent_history_cutover(
        &self,
        backup_digest: ContentDigest,
        backup_byte_count: u64,
        conversations: Vec<LegacyAgentHistoryConversationInput>,
    ) -> LegacyAgentHistoryCutoverProjection {
        let result = {
            let mut state = self.state();
            let Some(store) = state.store.clone() else {
                return LegacyAgentHistoryCutoverProjection::blocked(
                    StorageError::AgentTurnConflict,
                );
            };
            let input =
                match cutover_input(backup_digest, backup_byte_count, conversations, state.now()) {
                    Ok(input) => input,
                    Err(error) => return LegacyAgentHistoryCutoverProjection::blocked(error),
                };
            let result = store.stage_legacy_agent_history_cutover(input);
            if result.is_ok() {
                state.advance_revision();
            }
            result
        };
        self.agent_history_cutover_result(result)
    }

    pub fn verify_legacy_agent_history_cutover(
        &self,
        source_generation: u64,
    ) -> LegacyAgentHistoryCutoverProjection {
        let result = {
            let mut state = self.state();
            let Some(store) = state.store.clone() else {
                return LegacyAgentHistoryCutoverProjection::blocked(
                    StorageError::AgentTurnConflict,
                );
            };
            let result = store.verify_legacy_agent_history_cutover(source_generation, state.now());
            if result.is_ok() {
                state.advance_revision();
            }
            result
        };
        self.agent_history_cutover_result(result)
    }

    pub fn commit_legacy_agent_history_cutover(
        &self,
        source_generation: u64,
    ) -> LegacyAgentHistoryCutoverProjection {
        let result = {
            let mut state = self.state();
            let Some(store) = state.store.clone() else {
                return LegacyAgentHistoryCutoverProjection::blocked(
                    StorageError::AgentTurnConflict,
                );
            };
            let result = store.commit_legacy_agent_history_cutover(source_generation, state.now());
            if result.is_ok() {
                state.advance_revision();
            }
            result
        };
        self.agent_history_cutover_result(result)
    }

    pub fn discard_staged_legacy_agent_history_cutover(
        &self,
        source_generation: u64,
    ) -> LegacyAgentHistoryCutoverProjection {
        let result = {
            let mut state = self.state();
            let Some(store) = state.store.clone() else {
                return LegacyAgentHistoryCutoverProjection::blocked(
                    StorageError::AgentTurnConflict,
                );
            };
            match store.discard_staged_legacy_agent_history_cutover(source_generation) {
                Ok(_) => {
                    state.advance_revision();
                    store.agent_history_cutover_report()
                }
                Err(error) => Err(error),
            }
        };
        self.agent_history_cutover_result(result)
    }
}

impl Pod0Facade {
    fn agent_history_cutover_result(
        &self,
        result: Result<pod0_storage::LegacyAgentHistoryCutoverReport, StorageError>,
    ) -> LegacyAgentHistoryCutoverProjection {
        match result {
            Ok(report) => {
                self.notify_subscribers();
                LegacyAgentHistoryCutoverProjection::from_report(report)
            }
            Err(error) => LegacyAgentHistoryCutoverProjection::blocked(error),
        }
    }
}
