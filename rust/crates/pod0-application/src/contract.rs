use pod0_domain::{
    AutoDownloadPolicy, CancellationId, CommandId, EpisodeId, PodcastId, StateRevision,
};

pub const FACADE_CONTRACT_VERSION: u32 = 3;
pub const MAX_PROJECTION_ITEMS: u16 = 200;
pub const MAX_OPERATION_ITEMS: usize = 32;
pub const MAX_HOST_REQUEST_BATCH: u16 = 64;

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct CommandEnvelope {
    pub command_id: CommandId,
    pub cancellation_id: CancellationId,
    pub expected_revision: Option<StateRevision>,
    pub command: ApplicationCommand,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum ApplicationCommand {
    SubscribeToFeed {
        feed_url: String,
    },
    EnsurePodcast {
        feed_url: String,
    },
    RefreshPodcast {
        podcast_id: PodcastId,
    },
    HydratePodcastMetadata {
        podcast_id: PodcastId,
    },
    UpsertExternalEpisode {
        podcast_id: PodcastId,
        feed_url: Option<String>,
        podcast_title: String,
        audio_url: String,
        title: String,
        image_url: Option<String>,
        duration_milliseconds: Option<u64>,
    },
    Unsubscribe {
        podcast_id: PodcastId,
    },
    SetSubscriptionNotifications {
        podcast_id: PodcastId,
        enabled: bool,
    },
    SetSubscriptionAutoDownload {
        podcast_id: PodcastId,
        policy: AutoDownloadPolicy,
    },
    RequestPlayback {
        episode_id: EpisodeId,
    },
    CancelOperation {
        cancellation_id: CancellationId,
    },
    Unsupported {
        wire_code: u32,
    },
}
