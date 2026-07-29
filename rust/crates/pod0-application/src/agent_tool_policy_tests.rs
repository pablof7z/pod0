use super::*;
use pod0_domain::{AgentTurnId, StateRevision};

fn id<T>(value: u8, constructor: impl FnOnce([u8; 16]) -> T) -> T {
    constructor([value; 16])
}

#[test]
fn every_tool_has_one_policy_and_proposal_identity_is_deterministic() {
    assert_eq!(ALL_AGENT_TOOL_NAMES.len(), 48);
    let unique = ALL_AGENT_TOOL_NAMES
        .iter()
        .map(|(_, tool)| *tool)
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(unique.len(), 48);
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
