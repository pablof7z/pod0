use pod0_domain::{CommandId, FeedIdentityV1, PodcastId};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FeedIntent {
    Subscribe,
    Ensure,
    Refresh,
    Metadata,
}

#[derive(Clone, Debug)]
pub(super) struct PendingFeed {
    pub command_id: CommandId,
    pub fingerprint: String,
    pub intent: FeedIntent,
    pub feed_identity: FeedIdentityV1,
    pub podcast_id: PodcastId,
}
