use pod0_application::{AgentTurnProjection, AgentTurnState};
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

#[derive(Clone, Copy, Debug)]
pub struct AgentTurnMutation {
    pub expected_revision: StateRevision,
    pub audit_kind: AgentAuditKind,
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
