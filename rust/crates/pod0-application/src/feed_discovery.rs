use pod0_domain::{
    CancellationId, CommandId, EpisodeId, EpisodeRecord, FeedDiscoveryItemId,
    FeedDiscoveryOccurrenceId, HostRequestId, StateRevision,
};

use crate::download_contract::FramedHash;

pub const FEED_DISCOVERY_POLICY_VERSION: u32 = 1;
pub const FEED_DISCOVERY_WORKFLOW_SCHEMA_VERSION: u32 = 1;
pub const MAX_FEED_DISCOVERY_ITEMS: usize = 10_000;
pub const MAX_NEW_EPISODE_NOTIFICATIONS_PER_OCCURRENCE: usize = 3;
pub const FEED_DISCOVERY_NOTIFICATION_TTL_MILLISECONDS: i64 = 24 * 60 * 60 * 1_000;
pub const FEED_DISCOVERY_NOTIFICATION_RETRY_MILLISECONDS: i64 = 5 * 60 * 1_000;
pub const FEED_DISCOVERY_NOTIFICATION_MAX_ATTEMPTS: u8 = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Record)]
pub struct NewEpisodeNotificationSettingsProjection {
    pub enabled: bool,
    pub revision: StateRevision,
}

#[must_use]
pub fn feed_discovery_occurrence_id(command_id: CommandId) -> FeedDiscoveryOccurrenceId {
    let mut hash = FramedHash::new(b"pod0-feed-discovery-occurrence-v1");
    hash.bytes(&command_id.into_bytes());
    FeedDiscoveryOccurrenceId::from_bytes(hash.first_16())
}

#[must_use]
pub fn feed_discovery_item_id(
    occurrence_id: FeedDiscoveryOccurrenceId,
    episode_id: EpisodeId,
) -> FeedDiscoveryItemId {
    let mut hash = FramedHash::new(b"pod0-feed-discovery-item-v1");
    hash.bytes(&occurrence_id.into_bytes());
    hash.bytes(&episode_id.into_bytes());
    FeedDiscoveryItemId::from_bytes(hash.first_16())
}

#[must_use]
pub fn feed_discovery_download_command_id(
    occurrence_id: FeedDiscoveryOccurrenceId,
    episode_id: EpisodeId,
) -> CommandId {
    CommandId::from_bytes(feed_discovery_effect_bytes(
        b"pod0-feed-discovery-download-command-v1",
        occurrence_id,
        episode_id,
        0,
    ))
}

#[must_use]
pub fn feed_discovery_download_cancellation_id(
    occurrence_id: FeedDiscoveryOccurrenceId,
    episode_id: EpisodeId,
) -> CancellationId {
    CancellationId::from_bytes(feed_discovery_effect_bytes(
        b"pod0-feed-discovery-download-cancellation-v1",
        occurrence_id,
        episode_id,
        0,
    ))
}

#[must_use]
pub fn feed_discovery_notification_request_id(
    occurrence_id: FeedDiscoveryOccurrenceId,
    episode_id: EpisodeId,
    attempt: u8,
) -> HostRequestId {
    HostRequestId::from_bytes(feed_discovery_effect_bytes(
        b"pod0-feed-discovery-notification-request-v1",
        occurrence_id,
        episode_id,
        attempt,
    ))
}

#[must_use]
pub fn feed_discovery_notification_cancellation_id(
    occurrence_id: FeedDiscoveryOccurrenceId,
    episode_id: EpisodeId,
) -> CancellationId {
    CancellationId::from_bytes(feed_discovery_effect_bytes(
        b"pod0-feed-discovery-notification-cancellation-v1",
        occurrence_id,
        episode_id,
        0,
    ))
}

fn feed_discovery_effect_bytes(
    domain: &[u8],
    occurrence_id: FeedDiscoveryOccurrenceId,
    episode_id: EpisodeId,
    attempt: u8,
) -> [u8; 16] {
    let mut hash = FramedHash::new(domain);
    hash.bytes(&occurrence_id.into_bytes());
    hash.bytes(&episode_id.into_bytes());
    hash.bytes(&[attempt]);
    hash.first_16()
}

#[must_use]
pub fn feed_discovery_item_input_version(episode: &EpisodeRecord) -> String {
    let mut hash = FramedHash::new(b"pod0-feed-discovery-input-v1");
    hash.bytes(&episode.podcast_id.into_bytes());
    hash.bytes(&episode.episode_id.into_bytes());
    hash.string(&episode.publisher_guid);
    hash.string(&episode.title);
    hash.i64(episode.published_at.value);
    hash.string(&episode.enclosure_url);
    hash.string(episode.enclosure_mime_type.as_deref().unwrap_or_default());
    hash.u64(episode.duration_milliseconds.unwrap_or(0));
    hash.hex()
}

#[cfg(test)]
mod tests {
    use pod0_domain::{
        CompletionStatus, DownloadArtifactStatus, EpisodeFeedMetadata, EpisodeListeningState,
        PodcastId, TranscriptArtifactStatus, UnixTimestampMilliseconds,
    };

    use super::*;

    fn episode() -> EpisodeRecord {
        EpisodeRecord {
            episode_id: EpisodeId::from_parts(2, 1),
            podcast_id: PodcastId::from_parts(1, 1),
            publisher_guid: "guid".to_owned(),
            title: "Episode".to_owned(),
            description: String::new(),
            published_at: UnixTimestampMilliseconds::new(1_800_000_000_000),
            duration_milliseconds: Some(60_000),
            enclosure_url: "https://example.test/episode.mp3".to_owned(),
            enclosure_mime_type: Some("audio/mpeg".to_owned()),
            image_url: None,
            feed_metadata: EpisodeFeedMetadata::default(),
            listening: EpisodeListeningState {
                resume_position_milliseconds: 0,
                completion: CompletionStatus::InProgress,
            },
            is_starred: false,
            download: DownloadArtifactStatus::Unavailable,
            transcript: TranscriptArtifactStatus::Unavailable,
            generated_audio: None,
        }
    }

    #[test]
    fn identities_and_input_versions_are_stable_and_domain_separated() {
        let command = CommandId::from_parts(9, 1);
        let occurrence = feed_discovery_occurrence_id(command);
        let item = feed_discovery_item_id(occurrence, episode().episode_id);
        assert_eq!(occurrence, feed_discovery_occurrence_id(command));
        assert_eq!(
            item,
            feed_discovery_item_id(occurrence, episode().episode_id)
        );
        assert_ne!(occurrence.into_bytes(), item.into_bytes());

        let version = feed_discovery_item_input_version(&episode());
        assert_eq!(version.len(), 64);
        assert!(version.bytes().all(|byte| byte.is_ascii_hexdigit()));
        let mut changed = episode();
        changed.title = "Retitled".to_owned();
        assert_ne!(version, feed_discovery_item_input_version(&changed));
    }
}
