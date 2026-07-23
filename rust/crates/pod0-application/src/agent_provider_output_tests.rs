use super::*;
use pod0_domain::EpisodeId;

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
            "record_memory",
            r#"{"text":"Prefers primary sources"}"#,
        )),
        Ok(AgentToolAction::RecordMemory {
            text: "Prefers primary sources".into(),
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
    assert_eq!(
        parse_agent_tool_call(&call(
            "query_transcripts",
            r#"{"query":"habit cues","episode_id":"00000000-0000-0000-0000-000000000009","limit":4}"#,
        )),
        Ok(AgentToolAction::QueryTranscripts {
            query: "habit cues".into(),
            scope: RecallScope::Episode {
                episode_id: EpisodeId::from_parts(0, 9),
            },
            limit: 4,
        })
    );
    assert_eq!(
        parse_agent_tool_call(&call(
            "generate_tts_episode",
            r#"{"title":"Morning brief","script":"Here is your briefing.","voice_id":"voice-1"}"#,
        )),
        Ok(AgentToolAction::GenerateTtsEpisode {
            podcast_id: None,
            title: "Morning brief".into(),
            script: "Here is your briefing.".into(),
            voice_id: Some("voice-1".into()),
        })
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
        Err(AgentProviderOutputError::InvalidArguments)
    );
    assert_eq!(
        parse_agent_tool_call(&call(
            "query_transcripts",
            r#"{"query":"x","episode_id":"00000000-0000-0000-0000-000000000001","podcast_id":"00000000-0000-0000-0000-000000000002"}"#,
        )),
        Err(AgentProviderOutputError::InvalidArguments)
    );
}
