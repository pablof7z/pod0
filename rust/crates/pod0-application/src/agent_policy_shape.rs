use crate::AgentToolName;

pub(super) fn no_argument_tool(tool: AgentToolName) -> bool {
    use AgentToolName::*;
    matches!(
        tool,
        UpgradeThinking
            | ListScheduledTasks
            | ListConversations
            | PausePlayback
            | ListSubscriptions
            | ListPodcasts
            | ListCategories
            | ListInProgress
            | ListRecentUnplayed
            | ListAvailableVoices
            | ListMyPodcasts
    )
}

pub(super) fn text_tool(tool: AgentToolName) -> bool {
    matches!(tool, AgentToolName::UseSkill)
}

pub(super) fn search_tool(tool: AgentToolName) -> bool {
    use AgentToolName::*;
    matches!(
        tool,
        SearchConversations
            | SearchEpisodes
            | QueryTranscripts
            | PerplexitySearch
            | FindSimilarEpisodes
            | SearchPodcastDirectory
            | SearchYoutube
    )
}

pub(super) fn episode_tool(tool: AgentToolName) -> bool {
    use AgentToolName::*;
    matches!(
        tool,
        SummarizeEpisode
            | MarkEpisodePlayed
            | MarkEpisodeUnplayed
            | DownloadEpisode
            | RequestTranscription
            | DownloadAndTranscribe
    )
}

pub(super) fn podcast_tool(tool: AgentToolName) -> bool {
    use AgentToolName::*;
    matches!(
        tool,
        RefreshFeed | ListEpisodes | DeletePodcast | DeleteMyPodcast
    )
}
