use crate::agent_tool_catalog_builders::{
    boolean, decimal_permille, definition, integer, text, text_list,
};
use crate::{AgentToolName, MAX_AGENT_TOOLS_PER_TURN, MAX_CATEGORY_TAG_ITEMS};

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum AgentToolParameterKind {
    Text,
    Integer {
        minimum: i64,
        maximum: i64,
    },
    DecimalPermille {
        minimum: u16,
        maximum: u16,
    },
    Boolean,
    /// Array of strings. Present so a membership primitive can move many
    /// items in one call instead of forcing one tool call per item.
    TextList {
        maximum_items: u16,
    },
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
    AgentToolName::SearchPodcastDirectory,
    AgentToolName::PlayEpisode,
    AgentToolName::QueryTranscripts,
    AgentToolName::PausePlayback,
    AgentToolName::SetPlaybackRate,
    AgentToolName::GenerateTtsEpisode,
    AgentToolName::ListCategories,
    AgentToolName::WriteCategory,
    AgentToolName::TagItems,
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
    if let Some(definition) = crate::agent_tool_catalog_discovery::definition(tool) {
        return Some(definition);
    }
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
        SearchPodcastDirectory | PlayEpisode => unreachable!("handled above"),
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
        ListCategories => definition(
            tool,
            "list_categories",
            "List every category with its description, its tint, and the \
             shows and episodes it holds. Call this first so you work with \
             real category IDs instead of guessing at names.",
            Vec::new(),
        ),
        WriteCategory => definition(
            tool,
            "write_category",
            "Create, edit, or delete one category. Omit category_id to \
             create; pass it to edit; pass it with delete=true to remove. \
             Only the fields you supply change. A category is a named lens \
             the user can swipe to from Home — it can hold whole shows, \
             individual episodes, or both.",
            vec![
                text(
                    "category_id",
                    "Category UUID from list_categories. Omit to create a new category.",
                    false,
                ),
                text(
                    "name",
                    "Short display name, e.g. \"Marketing\" or \"Philosophy\". Required when creating.",
                    false,
                ),
                text(
                    "description",
                    "One sentence describing what belongs here. Shown on the category's own screen. Required when creating.",
                    false,
                ),
                text("color_hex", "Optional tint as #RRGGBB or #RRGGBBAA.", false),
                boolean(
                    "delete",
                    "Set true to delete the category. Its shows and episodes stay in the library.",
                    false,
                ),
            ],
        ),
        TagItems => definition(
            tool,
            "tag_items",
            "Add or remove library items in one category. An item is a \
             podcast or a single episode — pass whichever UUIDs you got \
             from the library tools and they are resolved automatically. \
             Tag a whole show when every episode fits the theme; tag single \
             episodes when only some do. Items can belong to several \
             categories at once.",
            vec![
                text("category_id", "Category UUID from list_categories.", true),
                text_list(
                    "add",
                    "Podcast or episode UUIDs to put in this category.",
                    MAX_CATEGORY_TAG_ITEMS,
                    false,
                ),
                text_list(
                    "remove",
                    "Podcast or episode UUIDs to take out of this category.",
                    MAX_CATEGORY_TAG_ITEMS,
                    false,
                ),
            ],
        ),
        _ => return None,
    };
    Some(definition)
}
