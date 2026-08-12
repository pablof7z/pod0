use pod0_application::{AgentConversationSummaryProjection, AgentTurnProjection, AgentTurnState};
use pod0_domain::{AgentTurnId, CommandId, StateRevision, UnixTimestampMilliseconds};

#[derive(Clone, Copy, Debug)]
pub struct AgentCommandContext {
    pub command_id: CommandId,
    pub command_fingerprint: [u8; 32],
    pub observed_at: UnixTimestampMilliseconds,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentAuditKind {
    Started,
    ModelObserved,
    AuthorizationObserved,
    ExecutionStarted,
    ActionObserved,
    Cancelled,
    Recovered,
}

impl AgentAuditKind {
    pub(crate) const fn code(self) -> &'static str {
        match self {
            Self::Started => "started",
            Self::ModelObserved => "model_observed",
            Self::AuthorizationObserved => "authorization_observed",
            Self::ExecutionStarted => "execution_started",
            Self::ActionObserved => "action_observed",
            Self::Cancelled => "cancelled",
            Self::Recovered => "recovered",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentMutationOutcome {
    Applied(AgentTurnState),
    Duplicate(AgentTurnState),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentTurnPage {
    pub items: Vec<AgentTurnProjection>,
    pub has_more: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentConversationPage {
    pub items: Vec<AgentConversationSummaryProjection>,
    pub has_more: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct AgentTurnMutation {
    pub expected_revision: StateRevision,
    pub audit_kind: AgentAuditKind,
}

#[derive(Clone, Debug)]
pub struct AgentModelObservationCommitInput {
    pub lease: pod0_application::PersistedEffectLeaseIdentity,
    pub observation: pod0_application::DurableAgentModelHostObservation,
    pub committed_at: UnixTimestampMilliseconds,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentModelObservationCommitOutcome {
    pub state: AgentTurnState,
    pub replayed: bool,
}

#[derive(Clone, Debug)]
pub struct AgentApprovalObservationCommitInput {
    pub lease: pod0_application::PersistedEffectLeaseIdentity,
    pub observation: pod0_application::DurableAgentApprovalHostObservation,
    pub committed_at: UnixTimestampMilliseconds,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentApprovalObservationCommitOutcome {
    pub state: AgentTurnState,
    pub replayed: bool,
}

#[derive(Clone, Debug)]
pub struct AgentCapabilityObservationCommitInput {
    pub lease: pod0_application::PersistedEffectLeaseIdentity,
    pub observation: pod0_application::DurableAgentCapabilityHostObservation,
    pub committed_at: UnixTimestampMilliseconds,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentCapabilityObservationCommitOutcome {
    pub state: AgentTurnState,
    pub replayed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AgentCancellationCommitOutcome {
    pub disposition: pod0_application::RequestDisposition,
    pub cancellation_id: Option<pod0_domain::CancellationId>,
    pub replayed: bool,
}

impl AgentMutationOutcome {
    #[must_use]
    pub fn state(&self) -> &AgentTurnState {
        match self {
            Self::Applied(state) | Self::Duplicate(state) => state,
        }
    }

    #[must_use]
    pub fn turn_id(&self) -> AgentTurnId {
        self.state().projection().turn_id
    }
}
