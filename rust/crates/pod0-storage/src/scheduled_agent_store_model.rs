use pod0_application::{
    ScheduledAgentExecutionObservation, ScheduledAgentExecutionRequest,
    ScheduledAgentOccurrenceState, ScheduledTaskDefinition,
};
use pod0_domain::{
    CancellationId, CommandId, HostRequestId, ScheduledOccurrenceId, ScheduledTaskId,
    StateRevision, UnixTimestampMilliseconds,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScheduledAgentAuthorityState {
    Inactive,
    Authoritative { source_generation: u64 },
}

impl ScheduledAgentAuthorityState {
    pub const fn is_authoritative(self) -> bool {
        matches!(self, Self::Authoritative { .. })
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ScheduledAgentCommandContext {
    pub command_id: CommandId,
    pub command_fingerprint: [u8; 32],
    pub cancellation_id: CancellationId,
    pub issued_revision: StateRevision,
    pub observed_at: UnixTimestampMilliseconds,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScheduledAgentHostRequestRecord {
    pub request_id: HostRequestId,
    pub command_id: CommandId,
    pub cancellation_id: CancellationId,
    pub issued_revision: StateRevision,
    pub deadline_at: UnixTimestampMilliseconds,
    pub execution: ScheduledAgentExecutionRequest,
    pub last_sequence_number: Option<u64>,
}

#[derive(Clone, Debug)]
pub struct ScheduledAgentObservationInput {
    pub request_id: HostRequestId,
    pub cancellation_id: CancellationId,
    pub observed_request_revision: StateRevision,
    pub sequence_number: u64,
    pub observed_at: UnixTimestampMilliseconds,
    pub observation: ScheduledAgentExecutionObservation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScheduledAgentObservationOutcome {
    Updated(ScheduledAgentOccurrenceState),
    Duplicate(ScheduledAgentOccurrenceState),
    Stale,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScheduledTaskPage {
    pub items: Vec<ScheduledTaskDefinition>,
    pub has_more: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScheduledOccurrencePage {
    pub items: Vec<ScheduledAgentOccurrenceState>,
    pub has_more: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScheduledAgentReconcileOutcome {
    pub created_occurrences: Vec<ScheduledOccurrenceId>,
    pub requests: Vec<ScheduledAgentHostRequestRecord>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScheduledAgentRecoveryReport {
    pub reissued_requests: Vec<ScheduledAgentHostRequestRecord>,
    pub ambiguous_occurrences: Vec<ScheduledOccurrenceId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScheduledTaskMutationOutcome {
    Applied(ScheduledTaskDefinition),
    Duplicate(ScheduledTaskDefinition),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScheduledTaskRemovalOutcome {
    Applied {
        task_id: ScheduledTaskId,
        revision: StateRevision,
    },
    Duplicate {
        task_id: ScheduledTaskId,
        revision: StateRevision,
    },
}
