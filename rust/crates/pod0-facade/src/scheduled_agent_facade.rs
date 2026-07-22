use crate::{ScheduledAgentExecutionObservation, ScheduledAgentExecutionRequest};

/// Converts bounded raw provider text into canonical Rust-owned artifact
/// evidence. Native callers cannot choose durable artifact identity or digest.
#[uniffi::export]
pub fn qualify_scheduled_agent_completion(
    execution: ScheduledAgentExecutionRequest,
    raw_output: String,
) -> Option<ScheduledAgentExecutionObservation> {
    pod0_application::qualify_scheduled_agent_completion(&execution, &raw_output)
}
