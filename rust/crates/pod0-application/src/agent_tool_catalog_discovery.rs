use crate::agent_tool_catalog_builders::{boolean, definition as make_definition, integer, text};
use crate::{AgentToolDefinition, AgentToolName};

pub(super) fn definition(tool: AgentToolName) -> Option<AgentToolDefinition> {
    let value = match tool {
        AgentToolName::SearchPodcastDirectory => make_definition(
            tool,
            "search_podcast_directory",
            "Search the public podcast catalog and podcast feeds for episodes, including episodes outside the user's library. Use this before play_episode when the requested episode is not already known.",
            vec![
                text(
                    "query",
                    "A fuzzy episode title, topic, guest, or description; exact wording is not required.",
                    true,
                ),
                text(
                    "scope",
                    "Optional podcast name or other show hint, such as the host or publisher.",
                    false,
                ),
                integer(
                    "limit",
                    "Maximum playable episode matches from 1 through 10. Defaults to 5.",
                    1,
                    10,
                    false,
                ),
                boolean(
                    "play",
                    "Set true when the user asked to play the best match immediately; leave false when they only asked to search.",
                    false,
                ),
            ],
        ),
        AgentToolName::PlayEpisode => make_definition(
            tool,
            "play_episode",
            "Play or queue an episode using an episode UUID returned by a library or public-catalog search tool.",
            vec![
                text(
                    "episode_id",
                    "Stable episode UUID returned by search_episodes, search_podcast_directory, or another library tool.",
                    true,
                ),
                integer(
                    "start_seconds",
                    "Optional position in seconds at which playback should begin.",
                    0,
                    604_800,
                    false,
                ),
                integer(
                    "end_seconds",
                    "Optional position in seconds at which bounded playback should stop.",
                    1,
                    604_800,
                    false,
                ),
                text(
                    "queue_position",
                    "Where to put the episode: now, next, or end. Defaults to now.",
                    false,
                ),
            ],
        ),
        _ => return None,
    };
    Some(value)
}
