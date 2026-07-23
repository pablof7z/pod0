use crate::{AgentToolName, MAX_AGENT_TOOLS_PER_TURN};

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum AgentToolParameterKind {
    Text,
    Integer { minimum: i64, maximum: i64 },
    DecimalPermille { minimum: u16, maximum: u16 },
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct AgentToolParameterDefinition {
    pub name: String,
    pub description: String,
    pub kind: AgentToolParameterKind,
    pub required: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct AgentToolDefinition {
    pub tool: AgentToolName,
    pub wire_name: String,
    pub description: String,
    pub parameters: Vec<AgentToolParameterDefinition>,
}

pub const PRODUCT_PROOF_AGENT_TOOLS: &[AgentToolName] = &[
    AgentToolName::CreateNote,
    AgentToolName::ListSubscriptions,
    AgentToolName::ListPodcasts,
    AgentToolName::ListEpisodes,
    AgentToolName::ListInProgress,
    AgentToolName::ListRecentUnplayed,
    AgentToolName::SearchEpisodes,
    AgentToolName::QueryTranscripts,
    AgentToolName::PausePlayback,
    AgentToolName::SetPlaybackRate,
    AgentToolName::GenerateTtsEpisode,
];

#[must_use]
pub fn product_proof_agent_tools() -> Vec<AgentToolName> {
    PRODUCT_PROOF_AGENT_TOOLS.to_vec()
}

#[must_use]
pub fn agent_tool_definitions(tools: &[AgentToolName]) -> Option<Vec<AgentToolDefinition>> {
    if tools.len() > MAX_AGENT_TOOLS_PER_TURN {
        return None;
    }
    tools.iter().copied().map(agent_tool_definition).collect()
}

#[must_use]
pub fn agent_tool_definition(tool: AgentToolName) -> Option<AgentToolDefinition> {
    use AgentToolName::*;
    let definition = match tool {
        CreateNote => definition(
            tool,
            "create_note",
            "Save a note or reflection for the user.",
            vec![text("text", "The note content to save.", true)],
        ),
        RecordMemory => definition(
            tool,
            "record_memory",
            "Remember a durable preference or fact for future conversations.",
            vec![text("text", "The preference or fact to remember.", true)],
        ),
        ListSubscriptions => definition(
            tool,
            "list_subscriptions",
            "List the podcasts the user currently subscribes to.",
            Vec::new(),
        ),
        ListPodcasts => definition(
            tool,
            "list_podcasts",
            "List every podcast currently known to the user's library.",
            Vec::new(),
        ),
        ListEpisodes => definition(
            tool,
            "list_episodes",
            "List episodes for one podcast, newest first.",
            vec![text(
                "podcast_id",
                "Stable podcast UUID returned by another library tool.",
                true,
            )],
        ),
        ListInProgress => definition(
            tool,
            "list_in_progress",
            "List episodes the user started but has not finished.",
            Vec::new(),
        ),
        ListRecentUnplayed => definition(
            tool,
            "list_recent_unplayed",
            "List recently published episodes the user has not played.",
            Vec::new(),
        ),
        SearchEpisodes => definition(
            tool,
            "search_episodes",
            "Search episode metadata in the user's library for topical or fuzzy recall.",
            vec![
                text("query", "Natural-language search query.", true),
                text(
                    "scope",
                    "Optional podcast UUID to constrain the search.",
                    false,
                ),
                integer(
                    "limit",
                    "Maximum results from 1 through 25. Defaults to 10.",
                    1,
                    25,
                    false,
                ),
            ],
        ),
        QueryTranscripts => definition(
            tool,
            "query_transcripts",
            "Search prepared transcripts and return exact timestamped evidence.",
            vec![
                text(
                    "query",
                    "Natural-language question to answer from transcripts.",
                    true,
                ),
                text(
                    "episode_id",
                    "Optional episode UUID to search within.",
                    false,
                ),
                text(
                    "podcast_id",
                    "Optional podcast UUID to search within.",
                    false,
                ),
                integer(
                    "limit",
                    "Maximum evidence spans from 1 through 8. Defaults to 8.",
                    1,
                    8,
                    false,
                ),
            ],
        ),
        PausePlayback => definition(
            tool,
            "pause_playback",
            "Pause current podcast playback and persist the playhead.",
            Vec::new(),
        ),
        SetPlaybackRate => definition(
            tool,
            "set_playback_rate",
            "Set the active podcast playback speed.",
            vec![decimal_permille(
                "rate",
                "Playback speed multiplier from 0.5 through 3.0.",
                500,
                3_000,
                true,
            )],
        ),
        GenerateTtsEpisode => definition(
            tool,
            "generate_tts_episode",
            "Create a durable playable audio episode from an approved script.",
            vec![
                text(
                    "podcast_id",
                    "Optional stable synthetic podcast UUID returned by another tool.",
                    false,
                ),
                text(
                    "title",
                    "Episode title shown in the library and player.",
                    true,
                ),
                text("script", "Complete narration script to synthesize.", true),
                text(
                    "voice_id",
                    "Optional configured ElevenLabs voice ID.",
                    false,
                ),
            ],
        ),
        _ => return None,
    };
    Some(definition)
}

fn definition(
    tool: AgentToolName,
    wire_name: &str,
    description: &str,
    parameters: Vec<AgentToolParameterDefinition>,
) -> AgentToolDefinition {
    AgentToolDefinition {
        tool,
        wire_name: wire_name.to_owned(),
        description: description.to_owned(),
        parameters,
    }
}

fn text(name: &str, description: &str, required: bool) -> AgentToolParameterDefinition {
    AgentToolParameterDefinition {
        name: name.to_owned(),
        description: description.to_owned(),
        kind: AgentToolParameterKind::Text,
        required,
    }
}

fn integer(
    name: &str,
    description: &str,
    minimum: i64,
    maximum: i64,
    required: bool,
) -> AgentToolParameterDefinition {
    AgentToolParameterDefinition {
        name: name.to_owned(),
        description: description.to_owned(),
        kind: AgentToolParameterKind::Integer { minimum, maximum },
        required,
    }
}

fn decimal_permille(
    name: &str,
    description: &str,
    minimum: u16,
    maximum: u16,
    required: bool,
) -> AgentToolParameterDefinition {
    AgentToolParameterDefinition {
        name: name.to_owned(),
        description: description.to_owned(),
        kind: AgentToolParameterKind::DecimalPermille { minimum, maximum },
        required,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AgentExecutionKind, agent_tool_policy, agent_tool_wire_name};
    use std::collections::BTreeSet;

    #[test]
    fn product_proof_catalog_is_unique_bounded_and_executable() {
        assert!(PRODUCT_PROOF_AGENT_TOOLS.len() <= MAX_AGENT_TOOLS_PER_TURN);
        let unique = PRODUCT_PROOF_AGENT_TOOLS
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        assert_eq!(unique.len(), PRODUCT_PROOF_AGENT_TOOLS.len());

        let definitions =
            agent_tool_definitions(PRODUCT_PROOF_AGENT_TOOLS).expect("complete catalog");
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
}
