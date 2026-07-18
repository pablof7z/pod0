use std::collections::BTreeMap;

use super::*;

fn fixture() -> BTreeMap<&'static str, &'static str> {
    include_str!("../../../../Fixtures/CoreListening/listening-domain-v1.properties")
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| line.split_once('=').expect("valid golden property"))
        .collect()
}

fn number<T: std::str::FromStr>(values: &BTreeMap<&str, &str>, key: &str) -> T {
    values[key]
        .parse()
        .unwrap_or_else(|_| panic!("valid {key}"))
}

fn id(values: &BTreeMap<&str, &str>, prefix: &str) -> (u64, u64) {
    (
        number(values, &format!("{prefix}_high")),
        number(values, &format!("{prefix}_low")),
    )
}

pub(crate) fn golden_snapshot() -> ListeningDomainSnapshot {
    let values = fixture();
    let (podcast_high, podcast_low) = id(&values, "podcast_id");
    let podcast_id = PodcastId::from_parts(podcast_high, podcast_low);
    let (episode_high, episode_low) = id(&values, "episode_id");
    let episode_id = EpisodeId::from_parts(episode_high, episode_low);
    let feed_identity = make_feed_identity_v1(values["feed_source_url"].to_owned()).unwrap();
    let artifact = |version_key, opaque_key| ArtifactReference {
        schema_version: number(&values, version_key),
        opaque_key: values[opaque_key].to_owned(),
    };
    let queue_id = |prefix| {
        let (high, low) = id(&values, prefix);
        QueueEntryId::from_parts(high, low)
    };

    ListeningDomainSnapshot {
        podcasts: vec![PodcastRecord {
            podcast_id,
            kind: PodcastKind::Rss,
            feed_identity: Some(feed_identity),
            title: values["podcast_title"].to_owned(),
            author: values["podcast_author"].to_owned(),
            image_url: Some(values["podcast_image_url"].to_owned()),
            description: values["podcast_description"].to_owned(),
            language: Some(values["podcast_language"].to_owned()),
            categories: values["podcast_categories"]
                .split(',')
                .map(str::to_owned)
                .collect(),
            discovered_at: UnixTimestampMilliseconds::new(number(
                &values,
                "podcast_discovered_at_ms",
            )),
            title_is_placeholder: false,
            last_refreshed_at: Some(UnixTimestampMilliseconds::new(number(
                &values,
                "podcast_last_refreshed_at_ms",
            ))),
            etag: Some(values["podcast_etag"].to_owned()),
            last_modified: Some(values["podcast_last_modified"].to_owned()),
        }],
        subscriptions: vec![PodcastSubscriptionRecord {
            podcast_id,
            subscribed_at: UnixTimestampMilliseconds::new(number(
                &values,
                "subscription_subscribed_at_ms",
            )),
            auto_download: AutoDownloadPolicy {
                mode: AutoDownloadMode::Latest {
                    count: number(&values, "auto_download_latest_count"),
                },
                wifi_only: true,
            },
            notifications_enabled: true,
            default_playback_rate: Some(PlaybackRatePermille {
                value: number(&values, "default_playback_rate_permille"),
            }),
        }],
        episodes: vec![EpisodeRecord {
            episode_id,
            podcast_id,
            publisher_guid: values["episode_guid"].to_owned(),
            title: values["episode_title"].to_owned(),
            description: values["episode_description"].to_owned(),
            published_at: UnixTimestampMilliseconds::new(number(
                &values,
                "episode_published_at_ms",
            )),
            duration_milliseconds: Some(number(&values, "episode_duration_ms")),
            enclosure_url: values["episode_enclosure_url"].to_owned(),
            enclosure_mime_type: Some(values["episode_enclosure_mime"].to_owned()),
            image_url: Some(values["episode_image_url"].to_owned()),
            feed_metadata: EpisodeFeedMetadata::default(),
            listening: EpisodeListeningState {
                resume_position_milliseconds: number(&values, "episode_resume_position_ms"),
                completion: CompletionStatus::InProgress,
            },
            is_starred: true,
            download: DownloadArtifactStatus::Available {
                reference: artifact("download_schema_version", "download_opaque_key"),
                byte_count: number(&values, "download_byte_count"),
            },
            transcript: TranscriptArtifactStatus::Available {
                reference: artifact("transcript_schema_version", "transcript_opaque_key"),
                source: TranscriptSource::Publisher,
            },
        }],
        playback: ListeningPlaybackPolicy {
            active_episode_id: Some(episode_id),
            queue: vec![
                QueueEntry {
                    queue_entry_id: queue_id("queue_whole_id"),
                    episode_id,
                    segment: None,
                    label: None,
                },
                QueueEntry {
                    queue_entry_id: queue_id("queue_segment_id"),
                    episode_id,
                    segment: Some(PlaybackSegment {
                        start_position_milliseconds: Some(number(
                            &values,
                            "queue_segment_start_ms",
                        )),
                        end_position_milliseconds: Some(number(&values, "queue_segment_end_ms")),
                    }),
                    label: Some(values["queue_segment_label"].to_owned()),
                },
            ],
            rate: PlaybackRatePermille {
                value: number(&values, "playback_rate_permille"),
            },
            sleep_mode: PlaybackSleepMode::Duration {
                duration_milliseconds: number(&values, "sleep_duration_ms"),
            },
            auto_mark_played_at_natural_end: true,
            auto_play_next: true,
            revision: StateRevision::new(number(&values, "state_revision")),
        },
    }
}

#[test]
fn golden_snapshot_round_trips_without_generating_or_rewriting_identity() {
    let snapshot = golden_snapshot();
    assert_eq!(
        validate_listening_snapshot(snapshot.clone()).unwrap(),
        snapshot
    );
    assert_eq!(fixture()["completion_percentage_threshold"], "none");
    assert_eq!(fixture()["unknown_future_field"], "ignored-by-v1-readers");
}

#[test]
fn feed_refresh_and_migration_retry_preserve_the_existing_podcast_id() {
    let values = fixture();
    let snapshot = golden_snapshot();
    let existing = PodcastIdentityRecord {
        podcast_id: snapshot.podcasts[0].podcast_id,
        feed_identity: snapshot.podcasts[0].feed_identity.clone().unwrap(),
    };
    let (incoming_high, incoming_low) = id(&values, "incoming_podcast_id");
    let incoming = PodcastId::from_parts(incoming_high, incoming_low);
    let resolve = || {
        resolve_podcast_identity_v1(
            incoming,
            values["feed_source_url"].to_owned(),
            vec![existing.clone()],
        )
    };

    assert_eq!(
        resolve().unwrap(),
        PodcastIdentityResolution::PreserveExisting {
            podcast_id: existing.podcast_id
        }
    );
    assert_eq!(resolve().unwrap(), resolve().unwrap());
    let distractor = PodcastIdentityRecord {
        podcast_id: PodcastId::from_parts(88, 89),
        feed_identity: make_feed_identity_v1("https://other.test/feed".to_owned()).unwrap(),
    };
    assert_eq!(
        resolve_podcast_identity_v1(
            incoming,
            values["feed_source_url"].to_owned(),
            vec![distractor, existing.clone()],
        )
        .unwrap(),
        resolve().unwrap()
    );
    assert!(matches!(
        resolve_podcast_identity_v1(
            incoming,
            format!("{}/", values["feed_source_url"]),
            vec![existing]
        )
        .unwrap(),
        PodcastIdentityResolution::AcceptIncoming { podcast_id } if podcast_id == incoming
    ));
}

#[test]
fn episode_guid_is_exact_and_scoped_to_its_parent_podcast() {
    let values = fixture();
    let snapshot = golden_snapshot();
    let existing = EpisodeIdentityRecord {
        episode_id: snapshot.episodes[0].episode_id,
        podcast_id: snapshot.episodes[0].podcast_id,
        publisher_guid: snapshot.episodes[0].publisher_guid.clone(),
    };
    let (incoming_high, incoming_low) = id(&values, "incoming_episode_id");
    let incoming = EpisodeId::from_parts(incoming_high, incoming_low);

    assert_eq!(
        resolve_episode_identity_v1(
            incoming,
            existing.podcast_id,
            existing.publisher_guid.clone(),
            vec![existing.clone()],
        )
        .unwrap(),
        EpisodeIdentityResolution::PreserveExisting {
            episode_id: existing.episode_id
        }
    );
    assert!(matches!(
        resolve_episode_identity_v1(
            incoming,
            PodcastId::from_parts(99, 100),
            existing.publisher_guid.clone(),
            vec![existing],
        )
        .unwrap(),
        EpisodeIdentityResolution::AcceptIncoming { episode_id } if episode_id == incoming
    ));
}

#[test]
fn modern_parent_wins_and_ambiguous_identity_fails_closed() {
    let modern = PodcastId::from_parts(1, 2);
    let legacy = PodcastId::from_parts(3, 4);
    assert_eq!(
        resolve_legacy_parent_id(Some(modern), Some(legacy)).unwrap(),
        modern
    );
    assert_eq!(
        resolve_legacy_parent_id(None, Some(legacy)).unwrap(),
        legacy
    );
    assert!(matches!(
        resolve_legacy_parent_id(None, None),
        Err(ListeningDomainError::MissingLegacyParentIdentity)
    ));

    let feed = make_feed_identity_v1("https://example.test/feed".to_owned()).unwrap();
    let error = resolve_podcast_identity_v1(
        PodcastId::from_parts(9, 9),
        feed.source_url.clone(),
        vec![
            PodcastIdentityRecord {
                podcast_id: PodcastId::from_parts(1, 1),
                feed_identity: feed.clone(),
            },
            PodcastIdentityRecord {
                podcast_id: PodcastId::from_parts(2, 2),
                feed_identity: feed,
            },
        ],
    );
    assert!(matches!(
        error,
        Err(ListeningDomainError::AmbiguousPodcastFeedIdentity)
    ));
}
