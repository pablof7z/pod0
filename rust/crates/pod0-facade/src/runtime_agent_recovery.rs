use pod0_storage::StorageError;

use crate::runtime_state::FacadeState;

impl FacadeState {
    pub(crate) fn rehydrate_agent_turns(&mut self) -> Result<(), StorageError> {
        // Durable effect intents and leases are the recovery authority. Do not
        // synthesize a second host queue or mutate agent state during startup.
        self.resume_agent_internal_commands();
        Ok(())
    }
}
