use pod0_domain::{
    CancellationId, CommandId, EpisodeId, FeedDiscoveryOccurrenceId, HostRequestId, PodcastId,
    StateRevision, UnixTimestampMilliseconds,
};

use crate::{HostRequest, HostRequestEnvelope, MAX_FEED_RESPONSE_BYTES};

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DurableFeedEffectRequest {
    pub request_id: HostRequestId,
    pub command_id: CommandId,
    pub cancellation_id: CancellationId,
    pub issued_revision: StateRevision,
    pub not_before: Option<UnixTimestampMilliseconds>,
    pub deadline_at: Option<UnixTimestampMilliseconds>,
    pub action: DurableFeedEffectAction,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum DurableFeedEffectAction {
    FetchFeed {
        podcast_id: PodcastId,
        feed_url: String,
        entity_tag: Option<String>,
        last_modified: Option<String>,
    },
    DeliverNewEpisodeNotification {
        occurrence_id: FeedDiscoveryOccurrenceId,
        episode_id: EpisodeId,
        podcast_id: PodcastId,
        podcast_title: String,
        episode_title: String,
    },
}

impl DurableFeedEffectRequest {
    #[must_use]
    pub const fn podcast_id(&self) -> PodcastId {
        match self.action {
            DurableFeedEffectAction::FetchFeed { podcast_id, .. }
            | DurableFeedEffectAction::DeliverNewEpisodeNotification { podcast_id, .. } => {
                podcast_id
            }
        }
    }

    #[must_use]
    pub const fn episode_id(&self) -> Option<EpisodeId> {
        match self.action {
            DurableFeedEffectAction::FetchFeed { .. } => None,
            DurableFeedEffectAction::DeliverNewEpisodeNotification { episode_id, .. } => {
                Some(episode_id)
            }
        }
    }

    #[must_use]
    pub fn to_host(&self) -> HostRequestEnvelope {
        let request = match &self.action {
            DurableFeedEffectAction::FetchFeed {
                feed_url,
                entity_tag,
                last_modified,
                ..
            } => HostRequest::FetchFeed {
                feed_url: feed_url.clone(),
                entity_tag: entity_tag.clone(),
                last_modified: last_modified.clone(),
                maximum_response_bytes: MAX_FEED_RESPONSE_BYTES,
            },
            DurableFeedEffectAction::DeliverNewEpisodeNotification {
                occurrence_id,
                episode_id,
                podcast_id,
                podcast_title,
                episode_title,
            } => HostRequest::DeliverNewEpisodeNotification {
                occurrence_id: *occurrence_id,
                episode_id: *episode_id,
                podcast_id: *podcast_id,
                podcast_title: podcast_title.clone(),
                episode_title: episode_title.clone(),
            },
        };
        HostRequestEnvelope {
            request_id: self.request_id,
            command_id: self.command_id,
            cancellation_id: self.cancellation_id,
            issued_revision: self.issued_revision,
            deadline_at: self.deadline_at,
            request,
        }
    }
}
