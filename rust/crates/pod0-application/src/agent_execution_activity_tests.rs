use pod0_domain::{
    ActivityCorrelationId, ActivityId, AgentTurnId, InternalCommandId, StateRevision,
};

use crate::{
    AgentExecutionActivityInput, AgentExecutionContinuation, RequestDisposition,
    plan_agent_execution,
};

#[test]
fn execution_can_only_begin_as_the_causal_internal_command() {
    let cause = ActivityId::from_parts(1, 2);
    let plan = plan_agent_execution(AgentExecutionActivityInput {
        internal_command_id: InternalCommandId::from_parts(2, 3),
        authorizing_activity_id: cause,
        correlation_id: ActivityCorrelationId::from_parts(3, 4),
        turn_id: AgentTurnId::from_parts(4, 5),
        current_revision: StateRevision::new(3),
        committed_revision: StateRevision::new(4),
        continuation: AgentExecutionContinuation::None,
    })
    .unwrap();
    let (_, _, _, facts, effects, commands, disposition) = plan.into_parts();
    assert_eq!(disposition, RequestDisposition::Accepted);
    assert_eq!(facts.len(), 2);
    assert!(
        facts
            .iter()
            .all(|fact| fact.caused_by_activity_id == Some(cause))
    );
    assert!(effects.is_empty());
    assert!(commands.is_empty());
}
