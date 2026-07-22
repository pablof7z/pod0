use pod0_domain::ContentDigest;
use pod0_storage::{
    LegacyScheduledAgentCutoverInput, StorageError, inspect_legacy_scheduled_agent_cutover,
};

use crate::scheduled_agent_cutover_mapping::cutover_input;
use crate::{
    LegacyScheduledAgentCutoverProjection, LegacyScheduledAgentOccurrenceInput,
    LegacyScheduledAgentTaskInput, Pod0Facade,
};

#[uniffi::export]
impl Pod0Facade {
    pub fn scheduled_agent_cutover(&self) -> LegacyScheduledAgentCutoverProjection {
        let state = self.state();
        let Some(store) = state.store.as_ref() else {
            return LegacyScheduledAgentCutoverProjection::blocked(
                StorageError::ScheduledAgentWorkflowConflict,
            );
        };
        store
            .scheduled_agent_cutover_report()
            .map(LegacyScheduledAgentCutoverProjection::from_report)
            .unwrap_or_else(LegacyScheduledAgentCutoverProjection::blocked)
    }

    pub fn inspect_legacy_scheduled_agent_cutover(
        &self,
        backup_digest: ContentDigest,
        backup_byte_count: u64,
        tasks: Vec<LegacyScheduledAgentTaskInput>,
        occurrences: Vec<LegacyScheduledAgentOccurrenceInput>,
    ) -> LegacyScheduledAgentCutoverProjection {
        let state = self.state();
        let input = match cutover_input(
            backup_digest,
            backup_byte_count,
            tasks,
            occurrences,
            state.now(),
        ) {
            Ok(input) => input,
            Err(error) => return LegacyScheduledAgentCutoverProjection::blocked(error),
        };
        inspected_projection(&input)
    }

    pub fn stage_legacy_scheduled_agent_cutover(
        &self,
        backup_digest: ContentDigest,
        backup_byte_count: u64,
        tasks: Vec<LegacyScheduledAgentTaskInput>,
        occurrences: Vec<LegacyScheduledAgentOccurrenceInput>,
    ) -> LegacyScheduledAgentCutoverProjection {
        let result = {
            let mut state = self.state();
            let Some(store) = state.store.clone() else {
                return LegacyScheduledAgentCutoverProjection::blocked(
                    StorageError::ScheduledAgentWorkflowConflict,
                );
            };
            let input = match cutover_input(
                backup_digest,
                backup_byte_count,
                tasks,
                occurrences,
                state.now(),
            ) {
                Ok(input) => input,
                Err(error) => return LegacyScheduledAgentCutoverProjection::blocked(error),
            };
            let result = store.stage_legacy_scheduled_agent_cutover(input);
            if result.is_ok() {
                state.advance_revision();
            }
            result
        };
        self.scheduled_cutover_result(result)
    }

    pub fn verify_legacy_scheduled_agent_cutover(
        &self,
        source_generation: u64,
    ) -> LegacyScheduledAgentCutoverProjection {
        let result = {
            let mut state = self.state();
            let Some(store) = state.store.clone() else {
                return LegacyScheduledAgentCutoverProjection::blocked(
                    StorageError::ScheduledAgentWorkflowConflict,
                );
            };
            let result =
                store.verify_legacy_scheduled_agent_cutover(source_generation, state.now());
            if result.is_ok() {
                state.advance_revision();
            }
            result
        };
        self.scheduled_cutover_result(result)
    }

    pub fn commit_legacy_scheduled_agent_cutover(
        &self,
        source_generation: u64,
    ) -> LegacyScheduledAgentCutoverProjection {
        let result = {
            let mut state = self.state();
            let Some(store) = state.store.clone() else {
                return LegacyScheduledAgentCutoverProjection::blocked(
                    StorageError::ScheduledAgentWorkflowConflict,
                );
            };
            let result =
                store.commit_legacy_scheduled_agent_cutover(source_generation, state.now());
            match result {
                Ok(report) => match store.scheduled_agent_store() {
                    Ok(scheduled_store) => {
                        state.scheduled_agent_store = Some(scheduled_store);
                        if let Err(error) = state.rehydrate_scheduled_agent_workflows() {
                            Err(error)
                        } else {
                            state.advance_revision();
                            Ok(report)
                        }
                    }
                    Err(error) => Err(error),
                },
                Err(error) => Err(error),
            }
        };
        self.scheduled_cutover_result(result)
    }

    pub fn discard_staged_legacy_scheduled_agent_cutover(
        &self,
        source_generation: u64,
    ) -> LegacyScheduledAgentCutoverProjection {
        let result = {
            let mut state = self.state();
            let Some(store) = state.store.clone() else {
                return LegacyScheduledAgentCutoverProjection::blocked(
                    StorageError::ScheduledAgentWorkflowConflict,
                );
            };
            match store.discard_staged_legacy_scheduled_agent_cutover(source_generation) {
                Ok(_) => {
                    state.advance_revision();
                    store.scheduled_agent_cutover_report()
                }
                Err(error) => Err(error),
            }
        };
        self.scheduled_cutover_result(result)
    }
}

impl Pod0Facade {
    fn scheduled_cutover_result(
        &self,
        result: Result<pod0_storage::LegacyScheduledAgentCutoverReport, StorageError>,
    ) -> LegacyScheduledAgentCutoverProjection {
        match result {
            Ok(report) => {
                self.notify_subscribers();
                LegacyScheduledAgentCutoverProjection::from_report(report)
            }
            Err(error) => LegacyScheduledAgentCutoverProjection::blocked(error),
        }
    }
}

fn inspected_projection(
    input: &LegacyScheduledAgentCutoverInput,
) -> LegacyScheduledAgentCutoverProjection {
    match inspect_legacy_scheduled_agent_cutover(input) {
        Ok((fingerprint, generation)) => LegacyScheduledAgentCutoverProjection::inspected(
            generation,
            fingerprint,
            input.backup_digest,
            input.backup_byte_count,
            input.tasks.len(),
            input.occurrences.len(),
        ),
        Err(error) => LegacyScheduledAgentCutoverProjection::blocked(error),
    }
}
