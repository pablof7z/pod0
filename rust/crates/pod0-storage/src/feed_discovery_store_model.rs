use pod0_domain::{
    CommandId, EpisodeId, FeedDiscoveryItemId, FeedDiscoveryOccurrenceId, PodcastId, StateRevision,
    UnixTimestampMilliseconds,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppliedFeed {
    pub revision: StateRevision,
    pub podcast_id: PodcastId,
    pub discovery_occurrence_id: Option<FeedDiscoveryOccurrenceId>,
    pub inserted_episode_count: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FeedDiscoveryItemRecord {
    pub item_id: FeedDiscoveryItemId,
    pub episode_id: EpisodeId,
    pub input_version: String,
    pub published_at: UnixTimestampMilliseconds,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FeedDiscoveryOccurrenceRecord {
    pub occurrence_id: FeedDiscoveryOccurrenceId,
    pub command_id: CommandId,
    pub podcast_id: PodcastId,
    pub workflow_schema_version: u32,
    pub policy_version: u32,
    pub is_initial_population: bool,
    pub observed_at: UnixTimestampMilliseconds,
    pub items: Vec<FeedDiscoveryItemRecord>,
}
