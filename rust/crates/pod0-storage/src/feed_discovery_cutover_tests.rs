use pod0_domain::{ContentDigest, UnixTimestampMilliseconds};
use rusqlite::Connection;

use crate::feed_discovery_store_test_support::*;
use crate::listening_import_test_support::id;
use crate::{
    FeedDiscoveryCutoverState, FeedDiscoveryEffectKind, FeedDiscoveryEffectStage,
    LegacyFeedDiscoveryCandidate, LegacyFeedDiscoveryCutoverInput,
    LegacyFeedDiscoveryDisposition, LegacyFeedDiscoveryEffectKind, LibraryStore, StorageError,
    inspect_legacy_feed_discovery_cutover,
};

#[test]
fn staged_legacy_work_is_inert_then_commits_once_and_recovers_after_restart() {
    let (fixture, store) = empty_authoritative_store();
    let podcast = podcast(&store);
    let episodes = (1..=3)
        .map(|value| {
            episode(
                podcast.podcast_id,
                value,
                BASE_TIME + i64::try_from(value).unwrap(),
            )
        })
        .collect::<Vec<_>>();
    store
        .apply_feed(
            id(600),
            &"6".repeat(64),
            podcast.clone(),
            episodes.clone(),
            true,
            false,
            None,
            None,
            BASE_TIME,
        )
        .unwrap();
    let command = id(610);
    let occurrence = pod0_application::feed_discovery_occurrence_id(command);
    let input = LegacyFeedDiscoveryCutoverInput {
        backup_digest: ContentDigest::from_bytes([7; 32]),
        backup_byte_count: 4_096,
        notification_command_id: id(611),
        notifications_enabled: true,
        inspected_job_count: 3,
        blocked_count: 0,
        ambiguous_count: 1,
        candidates: vec![
            candidate(
                occurrence,
                command,
                podcast.podcast_id,
                &episodes[0],
                LegacyFeedDiscoveryEffectKind::Download,
                LegacyFeedDiscoveryDisposition::Pending,
            ),
            candidate(
                occurrence,
                command,
                podcast.podcast_id,
                &episodes[1],
                LegacyFeedDiscoveryEffectKind::Notification,
                LegacyFeedDiscoveryDisposition::Ambiguous,
            ),
            candidate(
                occurrence,
                command,
                podcast.podcast_id,
                &episodes[2],
                LegacyFeedDiscoveryEffectKind::Notification,
                LegacyFeedDiscoveryDisposition::Pending,
            ),
        ],
        observed_at: time(BASE_TIME + 10),
    };
    assert_eq!(
        store.feed_discovery_cutover_report().unwrap().state,
        FeedDiscoveryCutoverState::NotStarted
    );
    let (fingerprint, generation) = inspect_legacy_feed_discovery_cutover(&input).unwrap();
    let staged = store
        .stage_legacy_feed_discovery_cutover(input.clone())
        .unwrap();
    assert_eq!(
        staged.state,
        FeedDiscoveryCutoverState::Staged {
            source_generation: generation
        }
    );
    assert_eq!(staged.source_fingerprint, Some(fingerprint));
    assert_eq!(staged.candidate_count, 3);
    assert!(store.pending_feed_discoveries(10).unwrap().is_empty());
    assert!(
        store
            .pending_feed_discovery_effects(
                FeedDiscoveryEffectKind::Download,
                BASE_TIME + 11,
                10
            )
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        store
            .stage_legacy_feed_discovery_cutover(input.clone())
            .unwrap(),
        staged
    );

    drop(store);
    let reopened = LibraryStore::open_authoritative(&fixture.target).unwrap();
    let committed = reopened
        .commit_legacy_feed_discovery_cutover(generation, time(BASE_TIME + 11))
        .unwrap();
    assert_eq!(
        committed.state,
        FeedDiscoveryCutoverState::Authoritative {
            source_generation: generation
        }
    );
    assert_eq!(
        reopened
            .commit_legacy_feed_discovery_cutover(generation, time(BASE_TIME + 12))
            .unwrap(),
        committed
    );
    assert!(
        reopened
            .new_episode_notification_settings()
            .unwrap()
            .enabled
    );
    let downloads = reopened
        .pending_feed_discovery_effects(FeedDiscoveryEffectKind::Download, BASE_TIME + 12, 10)
        .unwrap();
    let notifications = reopened
        .pending_feed_discovery_effects(FeedDiscoveryEffectKind::Notification, BASE_TIME + 12, 10)
        .unwrap();
    assert_eq!(downloads.len(), 1);
    assert_eq!(downloads[0].stage, FeedDiscoveryEffectStage::Pending);
    assert_eq!(notifications.len(), 1);
    assert_eq!(notifications[0].episode_id, episodes[2].episode_id);
    drop(reopened);

    let connection = Connection::open(&fixture.target).unwrap();
    let ambiguous: (String, Option<String>) = connection
        .query_row(
            "SELECT stage,failure_code FROM pod0_feed_discovery_effects
             WHERE occurrence_id=?1 AND episode_id=?2 AND kind='notification'",
            [
                occurrence.into_bytes().as_slice(),
                episodes[1].episode_id.into_bytes().as_slice(),
            ],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(
        ambiguous,
        (
            "succeeded".to_owned(),
            Some("legacy_ambiguous_delivery".to_owned())
        )
    );
}

#[test]
fn invalid_expired_ambiguous_or_missing_candidates_fail_closed_without_evidence() {
    let (_fixture, store) = empty_authoritative_store();
    let podcast = podcast(&store);
    let stored_episode = episode(podcast.podcast_id, 9, BASE_TIME);
    store
        .apply_feed(
            id(620),
            &"8".repeat(64),
            podcast.clone(),
            vec![stored_episode.clone()],
            true,
            false,
            None,
            None,
            BASE_TIME,
        )
        .unwrap();
    let command = id(621);
    let occurrence = pod0_application::feed_discovery_occurrence_id(command);
    let mut invalid = base_input(candidate(
        occurrence,
        command,
        podcast.podcast_id,
        &stored_episode,
        LegacyFeedDiscoveryEffectKind::Download,
        LegacyFeedDiscoveryDisposition::Ambiguous,
    ));
    invalid.ambiguous_count = 1;
    assert_eq!(
        inspect_legacy_feed_discovery_cutover(&invalid),
        Err(StorageError::InvalidFeedDiscoveryCutover)
    );

    invalid.candidates[0].disposition = LegacyFeedDiscoveryDisposition::Pending;
    invalid.ambiguous_count = 0;
    invalid.candidates[0].expires_at = time(BASE_TIME + 1);
    assert_eq!(
        store.stage_legacy_feed_discovery_cutover(invalid),
        Err(StorageError::InvalidFeedDiscoveryCutover)
    );

    let missing_episode = episode(podcast.podcast_id, 99, BASE_TIME);
    let missing = base_input(candidate(
        occurrence,
        command,
        podcast.podcast_id,
        &missing_episode,
        LegacyFeedDiscoveryEffectKind::Download,
        LegacyFeedDiscoveryDisposition::Pending,
    ));
    assert_eq!(
        store.stage_legacy_feed_discovery_cutover(missing),
        Err(StorageError::FeedDiscoveryCutoverConflict)
    );
    assert_eq!(
        store.feed_discovery_cutover_report().unwrap().state,
        FeedDiscoveryCutoverState::NotStarted
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
    disposition: LegacyFeedDiscoveryDisposition,
) -> LegacyFeedDiscoveryCandidate {
    LegacyFeedDiscoveryCandidate {
        occurrence_id,
        command_id,
        podcast_id,
        episode_id: episode.episode_id,
        kind,
        disposition,
        attempt: 0,
        not_before: (disposition == LegacyFeedDiscoveryDisposition::Pending)
            .then_some(time(BASE_TIME + 11)),
        observed_at: time(BASE_TIME + 2),
        expires_at: time(BASE_TIME + 100),
        published_at: episode.published_at,
        input_version: pod0_application::feed_discovery_item_input_version(episode),
    }
}

const fn time(value: i64) -> UnixTimestampMilliseconds {
    UnixTimestampMilliseconds::new(value)
}
