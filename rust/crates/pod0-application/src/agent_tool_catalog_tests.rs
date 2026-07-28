use std::collections::BTreeSet;

use crate::{
    AgentExecutionKind, AgentToolName, MAX_AGENT_TOOLS_PER_TURN, PRODUCT_PROOF_AGENT_TOOLS,
    agent_tool_definition, agent_tool_definitions, agent_tool_policy, agent_tool_wire_name,
};

#[test]
fn product_proof_catalog_is_unique_bounded_and_executable() {
    assert!(PRODUCT_PROOF_AGENT_TOOLS.len() <= MAX_AGENT_TOOLS_PER_TURN);
    let unique = PRODUCT_PROOF_AGENT_TOOLS
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    assert_eq!(unique.len(), PRODUCT_PROOF_AGENT_TOOLS.len());

    let definitions = agent_tool_definitions(PRODUCT_PROOF_AGENT_TOOLS).expect("complete catalog");
    assert_eq!(definitions.len(), PRODUCT_PROOF_AGENT_TOOLS.len());
    for definition in definitions {
        assert_eq!(definition.wire_name, agent_tool_wire_name(definition.tool));
        assert!(matches!(
            agent_tool_policy(definition.tool).execution,
            AgentExecutionKind::RustCommit
                | AgentExecutionKind::RustProjection
                | AgentExecutionKind::NativeCapability
        ));
        let parameter_names = definition
            .parameters
            .iter()
            .map(|parameter| parameter.name.as_str())
            .collect::<BTreeSet<_>>();
        assert_eq!(parameter_names.len(), definition.parameters.len());
    }
}

#[test]
fn deferred_tools_are_not_in_the_shipping_catalog() {
    assert!(!PRODUCT_PROOF_AGENT_TOOLS.contains(&AgentToolName::RecordMemory));
    assert!(agent_tool_definition(AgentToolName::ScheduleTask).is_none());
    assert!(agent_tool_definition(AgentToolName::PlayEpisode).is_none());
}
