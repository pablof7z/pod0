use super::*;

fn call(name: &str, arguments_json: &str) -> AgentModelToolCallObservation {
    AgentModelToolCallObservation {
        provider_call_id: "call-1".into(),
        tool_name: name.into(),
        arguments_json: arguments_json.into(),
    }
}

#[test]
fn parses_product_proof_actions_inside_rust() {
    assert_eq!(
        parse_agent_tool_call(&call("create_note", r#"{"text":"Architecture matters"}"#)),
        Ok(AgentToolAction::CreateNote {
            text: "Architecture matters".into(),
        })
    );
    assert_eq!(
        parse_agent_tool_call(&call(
            "search_episodes",
            r#"{"query":"architecture","limit":5}"#,
        )),
        Ok(AgentToolAction::Search {
            tool: AgentToolName::SearchEpisodes,
            query: "architecture".into(),
            scope: None,
            limit: 5,
        })
    );
    assert_eq!(
        parse_agent_tool_call(&call("set_playback_rate", r#"{"rate":1.25}"#)),
        Ok(AgentToolAction::SetPlaybackRate { permille: 1_250 })
    );
}

#[test]
fn rejects_unknown_malformed_and_not_yet_supported_actions() {
    assert_eq!(
        parse_agent_tool_call(&call("made_up", "{}")),
        Err(AgentProviderOutputError::UnsupportedTool)
    );
    assert_eq!(
        parse_agent_tool_call(&call("create_note", "not-json")),
        Err(AgentProviderOutputError::InvalidJson)
    );
    assert_eq!(
        parse_agent_tool_call(&call("generate_tts_episode", "{}")),
        Err(AgentProviderOutputError::UnsupportedTool)
    );
}
