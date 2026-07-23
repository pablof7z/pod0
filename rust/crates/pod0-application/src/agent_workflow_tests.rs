use super::*;
use pod0_domain::{
    AgentAuthorizationId, AgentExecutionFenceId, AgentTurnId, ConversationId, StateRevision,
    UnixTimestampMilliseconds,
};

fn id<T>(value: u8, constructor: impl FnOnce([u8; 16]) -> T) -> T {
    constructor([value; 16])
}
fn at(value: i64) -> UnixTimestampMilliseconds {
    UnixTimestampMilliseconds::new(value)
}

fn success(result: &str) -> AgentActionOutcome {
    AgentActionOutcome::Succeeded {
        bounded_result: result.into(),
        artifact_id: None,
        recall_evidence: Vec::new(),
    }
}

fn turn() -> AgentTurnState {
    AgentTurnState::start(AgentTurnStart {
        conversation_id: id(1, ConversationId::from_bytes),
        turn_id: id(2, AgentTurnId::from_bytes),
        model_fence_id: id(3, AgentExecutionFenceId::from_bytes),
        user_input: "Create a note saying architecture matters".into(),
        model_reference: "openrouter/test".into(),
        available_tools: vec![AgentToolName::CreateNote],
        cancellation_id: pod0_domain::CancellationId::from_parts(1, 2),
        observed_at: at(10),
    })
    .unwrap()
}

fn propose_note(state: &mut AgentTurnState) -> AgentProposalProjection {
    assert_eq!(
        state.observe_model(AgentModelObservation {
            turn_id: id(2, AgentTurnId::from_bytes),
            model_fence_id: id(3, AgentExecutionFenceId::from_bytes),
            assistant_text: "I'll save that.".into(),
            proposed_action: Some(AgentToolAction::CreateNote {
                text: "Architecture matters".into()
            }),
            usage: None,
            observed_at: at(20),
        }),
        AgentWorkflowAcceptance::Updated
    );
    state.projection().proposal.unwrap()
}

#[test]
fn invalid_action_fails_before_authorization_or_execution() {
    let mut state = turn();
    let result = state.observe_model(AgentModelObservation {
        turn_id: id(2, AgentTurnId::from_bytes),
        model_fence_id: id(3, AgentExecutionFenceId::from_bytes),
        assistant_text: String::new(),
        proposed_action: Some(AgentToolAction::Search {
            tool: AgentToolName::CreateNote,
            query: "x".into(),
            scope: None,
            limit: 1,
        }),
        usage: None,
        observed_at: at(20),
    });
    assert_eq!(result, AgentWorkflowAcceptance::Rejected);
    assert_eq!(state.projection().stage, AgentTurnStage::Failed);
}

#[test]
fn matching_durable_authority_is_required_and_denial_is_terminal() {
    let mut state = turn();
    let proposal = propose_note(&mut state);
    assert_eq!(state.projection().stage, AgentTurnStage::ApprovalRequired);
    let wrong = state.authorize(AgentAuthorizationObservation {
        proposal_id: proposal.proposal_id,
        proposal_digest: proposal.proposal_digest,
        authority: AgentAuthority::OneShotApproval,
        authorization_id: id(4, AgentAuthorizationId::from_bytes),
        approved: true,
        observed_at: at(30),
    });
    assert_eq!(wrong, AgentWorkflowAcceptance::Rejected);
    let denied = state.authorize(AgentAuthorizationObservation {
        proposal_id: proposal.proposal_id,
        proposal_digest: proposal.proposal_digest,
        authority: AgentAuthority::DurableTurnGrant,
        authorization_id: id(5, AgentAuthorizationId::from_bytes),
        approved: false,
        observed_at: at(31),
    });
    assert_eq!(denied, AgentWorkflowAcceptance::Updated);
    assert_eq!(state.projection().stage, AgentTurnStage::Denied);
}

#[test]
fn stale_fences_and_duplicate_observations_cannot_repeat_a_commit() {
    let mut state = turn();
    let proposal = propose_note(&mut state);
    let authorization = AgentAuthorizationObservation {
        proposal_id: proposal.proposal_id,
        proposal_digest: proposal.proposal_digest,
        authority: AgentAuthority::DurableTurnGrant,
        authorization_id: id(4, AgentAuthorizationId::from_bytes),
        approved: true,
        observed_at: at(30),
    };
    assert_eq!(
        state.authorize(authorization.clone()),
        AgentWorkflowAcceptance::Updated
    );
    assert_eq!(
        state.authorize(authorization),
        AgentWorkflowAcceptance::Duplicate
    );
    let fence = id(6, AgentExecutionFenceId::from_bytes);
    assert_eq!(
        state.begin_execution(fence, at(40)),
        AgentWorkflowAcceptance::Updated
    );
    let stale = AgentActionObservation {
        proposal_id: proposal.proposal_id,
        execution_fence_id: id(7, AgentExecutionFenceId::from_bytes),
        outcome: success("saved"),
        observed_at: at(50),
    };
    assert_eq!(state.observe_action(stale), AgentWorkflowAcceptance::Stale);
    let completed = AgentActionObservation {
        execution_fence_id: fence,
        ..AgentActionObservation {
            proposal_id: proposal.proposal_id,
            execution_fence_id: fence,
            outcome: success("saved"),
            observed_at: at(50),
        }
    };
    assert_eq!(
        state.observe_action(completed.clone()),
        AgentWorkflowAcceptance::Updated
    );
    let commit = state.projection().commit.unwrap();
    assert_eq!(
        state.observe_action(completed),
        AgentWorkflowAcceptance::Duplicate
    );
    assert_eq!(state.projection().commit.unwrap(), commit);
}

#[test]
fn committed_action_continues_once_for_a_final_answer_without_more_tools() {
    let mut state = turn();
    let proposal = propose_note(&mut state);
    state.authorize(AgentAuthorizationObservation {
        proposal_id: proposal.proposal_id,
        proposal_digest: proposal.proposal_digest,
        authority: AgentAuthority::DurableTurnGrant,
        authorization_id: id(4, AgentAuthorizationId::from_bytes),
        approved: true,
        observed_at: at(30),
    });
    let action_fence = id(6, AgentExecutionFenceId::from_bytes);
    state.begin_execution(action_fence, at(40));
    state.observe_action(AgentActionObservation {
        proposal_id: proposal.proposal_id,
        execution_fence_id: action_fence,
        outcome: success(r#"{"saved":true}"#),
        observed_at: at(50),
    });
    let model_fence = id(8, AgentExecutionFenceId::from_bytes);

    assert_eq!(
        state.continue_after_commit(model_fence, at(51)),
        AgentWorkflowAcceptance::Updated
    );
    assert_eq!(state.projection().stage, AgentTurnStage::AwaitingModel);
    assert!(state.projection().commit.is_some());
    assert_eq!(
        state.observe_model(AgentModelObservation {
            turn_id: id(2, AgentTurnId::from_bytes),
            model_fence_id: model_fence,
            assistant_text: "Saved that note.".into(),
            proposed_action: None,
            usage: None,
            observed_at: at(60),
        }),
        AgentWorkflowAcceptance::Updated
    );
    assert_eq!(state.projection().stage, AgentTurnStage::Completed);
    assert_eq!(
        state.projection().messages.last().unwrap().content,
        "Saved that note."
    );
}

#[test]
fn post_commit_continuation_rejects_a_second_tool_action() {
    let mut state = turn();
    let proposal = propose_note(&mut state);
    state.authorize(AgentAuthorizationObservation {
        proposal_id: proposal.proposal_id,
        proposal_digest: proposal.proposal_digest,
        authority: AgentAuthority::DurableTurnGrant,
        authorization_id: id(4, AgentAuthorizationId::from_bytes),
        approved: true,
        observed_at: at(30),
    });
    let action_fence = id(6, AgentExecutionFenceId::from_bytes);
    state.begin_execution(action_fence, at(40));
    state.observe_action(AgentActionObservation {
        proposal_id: proposal.proposal_id,
        execution_fence_id: action_fence,
        outcome: success("saved"),
        observed_at: at(50),
    });
    let model_fence = id(8, AgentExecutionFenceId::from_bytes);
    state.continue_after_commit(model_fence, at(51));

    assert_eq!(
        state.observe_model(AgentModelObservation {
            turn_id: id(2, AgentTurnId::from_bytes),
            model_fence_id: model_fence,
            assistant_text: String::new(),
            proposed_action: Some(AgentToolAction::CreateNote {
                text: "second write".into(),
            }),
            usage: None,
            observed_at: at(60),
        }),
        AgentWorkflowAcceptance::Rejected
    );
    assert_eq!(state.projection().stage, AgentTurnStage::Failed);
}

#[test]
fn cancellation_and_provider_failure_are_explicit_states() {
    let mut cancelled = turn();
    assert_eq!(cancelled.cancel(at(12)), AgentWorkflowAcceptance::Updated);
    assert_eq!(cancelled.projection().stage, AgentTurnStage::Cancelled);

    let mut failed = turn();
    let proposal = propose_note(&mut failed);
    failed.authorize(AgentAuthorizationObservation {
        proposal_id: proposal.proposal_id,
        proposal_digest: proposal.proposal_digest,
        authority: AgentAuthority::DurableTurnGrant,
        authorization_id: id(4, AgentAuthorizationId::from_bytes),
        approved: true,
        observed_at: at(30),
    });
    let fence = id(6, AgentExecutionFenceId::from_bytes);
    failed.begin_execution(fence, at(40));
    failed.observe_action(AgentActionObservation {
        proposal_id: proposal.proposal_id,
        execution_fence_id: fence,
        outcome: AgentActionOutcome::Failed {
            safe_detail: Some("provider unavailable".into()),
        },
        observed_at: at(50),
    });
    assert_eq!(failed.projection().stage, AgentTurnStage::Failed);
    assert_eq!(
        failed.projection().safe_failure.as_deref(),
        Some("provider unavailable")
    );
}

#[test]
fn every_tool_has_one_policy_and_proposal_identity_is_deterministic() {
    assert_eq!(ALL_AGENT_TOOL_NAMES.len(), 46);
    let unique = ALL_AGENT_TOOL_NAMES
        .iter()
        .map(|(_, tool)| *tool)
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(unique.len(), 46);
    for (_, tool) in ALL_AGENT_TOOL_NAMES {
        assert_eq!(agent_tool_policy(*tool).tool, *tool);
    }
    let action = AgentToolAction::CreateNote {
        text: "same".into(),
    };
    assert_eq!(
        agent_proposal_identity(
            id(2, AgentTurnId::from_bytes),
            StateRevision::new(2),
            &action
        ),
        agent_proposal_identity(
            id(2, AgentTurnId::from_bytes),
            StateRevision::new(2),
            &action
        )
    );
}
