use pod0_domain::{ContentDigest, UnixTimestampMilliseconds};

use crate::feed_discovery_store_test_support::*;
use crate::listening_import_test_support::id;
use crate::{
    LegacyFeedDiscoveryCandidate, LegacyFeedDiscoveryCutoverInput,
    LegacyFeedDiscoveryDisposition, LegacyFeedDiscoveryEffectKind, StorageError,
    inspect_legacy_feed_discovery_cutover,
};

#[test]
fn blocked_effect_count_may_exceed_the_number_of_source_jobs() {
    let input = LegacyFeedDiscoveryCutoverInput {
        backup_digest: ContentDigest::from_bytes([10; 32]),
        backup_byte_count: 100,
        notification_command_id: id(698),
        notifications_enabled: true,
        inspected_job_count: 1,
        blocked_count: 3,
        ambiguous_count: 0,
        candidates: vec![],
        observed_at: time(BASE_TIME + 10),
    };
    assert!(inspect_legacy_feed_discovery_cutover(&input).is_ok());

    let invalid = LegacyFeedDiscoveryCutoverInput {
        inspected_job_count: 0,
        ..input
    };
    assert_eq!(
        inspect_legacy_feed_discovery_cutover(&invalid),
        Err(StorageError::InvalidFeedDiscoveryCutover)
    );
}

#[test]
fn duplicate_episode_effects_must_describe_the_same_item() {
    let (_fixture, store) = empty_authoritative_store();
    let podcast = podcast(&store);
    let stored_episode = episode(podcast.podcast_id, 11, BASE_TIME);
    let command = id(630);
    let occurrence = pod0_application::feed_discovery_occurrence_id(command);
    let download = candidate(
        occurrence,
        command,
        podcast.podcast_id,
        &stored_episode,
        LegacyFeedDiscoveryEffectKind::Download,
    );
    let mut notification = candidate(
        occurrence,
        command,
        podcast.podcast_id,
        &stored_episode,
        LegacyFeedDiscoveryEffectKind::Notification,
    );
    notification.input_version = "a".repeat(64);
    let mut input = base_input(download);
    input.candidates.push(notification);

    assert_eq!(
        inspect_legacy_feed_discovery_cutover(&input),
        Err(StorageError::InvalidFeedDiscoveryCutover)
    );
}

fn base_input(candidate: LegacyFeedDiscoveryCandidate) -> LegacyFeedDiscoveryCutoverInput {
    LegacyFeedDiscoveryCutoverInput {
        backup_digest: ContentDigest::from_bytes([9; 32]),
        backup_byte_count: 100,
        notification_command_id: id(699),
        notifications_enabled: true,
        inspected_job_count: 1,
        blocked_count: 0,
        ambiguous_count: 0,
        candidates: vec![candidate],
        observed_at: time(BASE_TIME + 10),
    }
}

fn candidate(
    occurrence_id: pod0_domain::FeedDiscoveryOccurrenceId,
    command_id: pod0_domain::CommandId,
    podcast_id: pod0_domain::PodcastId,
    episode: &pod0_domain::EpisodeRecord,
    kind: LegacyFeedDiscoveryEffectKind,
) -> LegacyFeedDiscoveryCandidate {
    LegacyFeedDiscoveryCandidate {
        occurrence_id,
        command_id,
        podcast_id,
        episode_id: episode.episode_id,
        kind,
        disposition: LegacyFeedDiscoveryDisposition::Pending,
        attempt: 0,
        not_before: Some(time(BASE_TIME + 11)),
        observed_at: time(BASE_TIME + 2),
        expires_at: time(BASE_TIME + 100),
        published_at: episode.published_at,
        input_version: pod0_application::feed_discovery_item_input_version(episode),
    }
}

const fn time(value: i64) -> UnixTimestampMilliseconds {
    UnixTimestampMilliseconds::new(value)
}
