use pod0_domain::{
    AutoDownloadMode, AutoDownloadPolicy, EpisodeId, FeedIdentityV1, ListeningDomainSnapshot,
    ListeningPlaybackPolicy, PlaybackSleepMode, PodcastId, PodcastKind, PodcastRecord,
    PodcastSubscriptionRecord, StateRevision, UnixTimestampMilliseconds, make_feed_identity_v1,
    validate_listening_snapshot,
};
use serde_json::Value;

use crate::import_model::InspectedLegacySource;
use crate::legacy_episode::episodes;
use crate::legacy_format::{
    RawAppState, RawAutoDownload, RawPodcast, RawSubscription, checked_count, enum_variant,
    playback_rate, timestamp_milliseconds, unknown_wire_code, uuid_bytes,
};
use crate::{LegacyImportPlan, LegacySourceKind, StorageError};

pub(crate) fn transform_source(
    raw: RawAppState,
    sqlite_episode_payloads: Option<Vec<Vec<u8>>>,
    source_kind: LegacySourceKind,
    source_hash: String,
) -> Result<InspectedLegacySource, StorageError> {
    let episode_payloads = if let Some(payloads) = sqlite_episode_payloads {
        payloads
    } else {
        raw.episodes
            .iter()
            .map(|value| {
                serde_json::to_vec(value).map_err(|_| StorageError::InvalidLegacyRecord {
                    entity: "episode",
                    index: 0,
                    detail: "episode payload cannot be serialized",
                })
            })
            .collect::<Result<_, _>>()?
    };
    let podcasts = podcasts(&raw)?;
    let subscriptions = subscriptions(&raw)?;
    let episodes = episodes(&episode_payloads)?;
    let active_episode_id = raw
        .last_played_episode_id
        .as_deref()
        .map(|value| uuid_bytes(value, "playback", 0).map(EpisodeId::from_bytes))
        .transpose()?
        .filter(|episode_id| {
            episodes
                .iter()
                .any(|episode| episode.episode_id == *episode_id)
        });
    let revision = raw.generation.max(1);
    let snapshot = ListeningDomainSnapshot {
        podcasts,
        subscriptions,
        episodes,
        playback: ListeningPlaybackPolicy {
            active_episode_id,
            active_segment: None,
            active_label: None,
            queue: Vec::new(),
            rate: playback_rate(
                raw.settings.default_playback_rate.unwrap_or(1.0),
                "settings",
                0,
            )?,
            sleep_mode: PlaybackSleepMode::Off,
            auto_mark_played_at_natural_end: raw.settings.auto_mark_played_at_end.unwrap_or(true),
            auto_play_next: raw.settings.auto_play_next.unwrap_or(true),
            revision: StateRevision::new(revision),
        },
    };
    let snapshot =
        validate_listening_snapshot(snapshot).map_err(|_| StorageError::InvalidLegacyRecord {
            entity: "snapshot",
            index: 0,
            detail: "records violate listening-domain identity or state invariants",
        })?;
    let plan = LegacyImportPlan {
        source_kind,
        source_hash,
        source_generation: raw.generation,
        podcast_count: checked_count(snapshot.podcasts.len(), "podcast")?,
        subscription_count: checked_count(snapshot.subscriptions.len(), "subscription")?,
        episode_count: checked_count(snapshot.episodes.len(), "episode")?,
    };
    Ok(InspectedLegacySource {
        plan,
        snapshot,
        episode_payloads,
    })
}

fn podcasts(raw: &RawAppState) -> Result<Vec<PodcastRecord>, StorageError> {
    if let Some(podcasts) = &raw.podcasts {
        return podcasts
            .iter()
            .enumerate()
            .map(|(index, podcast)| podcast_record(podcast, checked_count(index, "row")?))
            .collect();
    }
    raw.subscriptions
        .iter()
        .enumerate()
        .map(|(index, subscription)| legacy_podcast(subscription, checked_count(index, "row")?))
        .collect()
}

fn podcast_record(raw: &RawPodcast, index: u32) -> Result<PodcastRecord, StorageError> {
    let kind = podcast_kind(raw.kind.as_deref().unwrap_or("rss"));
    let feed_identity = feed_identity(raw.feed_url.as_deref(), &kind, index)?;
    Ok(PodcastRecord {
        podcast_id: PodcastId::from_bytes(uuid_bytes(&raw.id, "podcast", index)?),
        kind,
        feed_identity,
        title: raw.title.clone(),
        author: raw.author.clone(),
        image_url: raw.image_url.clone(),
        description: raw.description.clone(),
        language: raw.language.clone(),
        categories: raw.categories.clone(),
        discovered_at: UnixTimestampMilliseconds::new(timestamp_milliseconds(
            raw.discovered_at.as_ref(),
            "podcast",
            index,
        )?),
        title_is_placeholder: raw.title_is_placeholder,
        last_refreshed_at: raw
            .last_refreshed_at
            .as_ref()
            .map(|value| {
                timestamp_milliseconds(Some(value), "podcast", index)
                    .map(UnixTimestampMilliseconds::new)
            })
            .transpose()?,
        etag: raw.etag.clone(),
        last_modified: raw.last_modified.clone(),
    })
}

fn legacy_podcast(raw: &RawSubscription, index: u32) -> Result<PodcastRecord, StorageError> {
    let id = raw.id.as_deref().ok_or(StorageError::InvalidLegacyRecord {
        entity: "subscription",
        index,
        detail: "legacy subscription has no identity",
    })?;
    let kind = if raw.is_agent_generated.unwrap_or(false) {
        PodcastKind::Synthetic
    } else {
        PodcastKind::Rss
    };
    Ok(PodcastRecord {
        podcast_id: PodcastId::from_bytes(uuid_bytes(id, "subscription", index)?),
        feed_identity: feed_identity(raw.feed_url.as_deref(), &kind, index)?,
        kind,
        title: raw.title.clone().unwrap_or_default(),
        author: raw.author.clone().unwrap_or_default(),
        image_url: raw.image_url.clone(),
        description: raw.description.clone().unwrap_or_default(),
        language: raw.language.clone(),
        categories: raw.categories.clone().unwrap_or_default(),
        discovered_at: UnixTimestampMilliseconds::new(timestamp_milliseconds(
            raw.subscribed_at.as_ref(),
            "subscription",
            index,
        )?),
        title_is_placeholder: false,
        last_refreshed_at: raw
            .last_refreshed_at
            .as_ref()
            .map(|value| {
                timestamp_milliseconds(Some(value), "subscription", index)
                    .map(UnixTimestampMilliseconds::new)
            })
            .transpose()?,
        etag: raw.etag.clone(),
        last_modified: raw.last_modified.clone(),
    })
}

fn subscriptions(raw: &RawAppState) -> Result<Vec<PodcastSubscriptionRecord>, StorageError> {
    raw.subscriptions
        .iter()
        .enumerate()
        .filter(|(_, row)| raw.podcasts.is_some() || !row.is_agent_generated.unwrap_or(false))
        .map(|(index, row)| subscription(row, checked_count(index, "row")?))
        .collect()
}

fn subscription(
    raw: &RawSubscription,
    index: u32,
) -> Result<PodcastSubscriptionRecord, StorageError> {
    let id =
        raw.podcast_id
            .as_ref()
            .or(raw.id.as_ref())
            .ok_or(StorageError::InvalidLegacyRecord {
                entity: "subscription",
                index,
                detail: "subscription has no podcast identity",
            })?;
    let auto_download = auto_download(raw.auto_download.as_ref(), index)?;
    Ok(PodcastSubscriptionRecord {
        podcast_id: PodcastId::from_bytes(uuid_bytes(id, "subscription", index)?),
        subscribed_at: UnixTimestampMilliseconds::new(timestamp_milliseconds(
            raw.subscribed_at.as_ref(),
            "subscription",
            index,
        )?),
        auto_download,
        notifications_enabled: raw.notifications_enabled.unwrap_or(true),
        default_playback_rate: raw
            .default_playback_rate
            .map(|value| playback_rate(value, "subscription", index))
            .transpose()?,
    })
}

fn podcast_kind(value: &str) -> PodcastKind {
    match value {
        "rss" => PodcastKind::Rss,
        "synthetic" => PodcastKind::Synthetic,
        other => PodcastKind::Unsupported {
            wire_code: unknown_wire_code(other),
        },
    }
}

fn feed_identity(
    value: Option<&str>,
    kind: &PodcastKind,
    index: u32,
) -> Result<Option<FeedIdentityV1>, StorageError> {
    // Ignore the old private `agent-generated://` non-feed sentinel.
    if matches!(kind, PodcastKind::Synthetic) {
        return Ok(None);
    }
    let Some(value) = value else {
        return if matches!(kind, PodcastKind::Rss) {
            Err(StorageError::InvalidLegacyRecord {
                entity: "podcast",
                index,
                detail: "RSS podcast has no feed URL",
            })
        } else {
            Ok(None)
        };
    };
    make_feed_identity_v1(value.to_owned())
        .map(Some)
        .map_err(|_| StorageError::InvalidLegacyRecord {
            entity: "podcast",
            index,
            detail: "feed URL is invalid",
        })
}

fn auto_download(
    raw: Option<&RawAutoDownload>,
    index: u32,
) -> Result<AutoDownloadPolicy, StorageError> {
    let Some(raw) = raw else {
        return Ok(AutoDownloadPolicy {
            mode: AutoDownloadMode::AllNew,
            wifi_only: true,
        });
    };
    let (name, payload) = enum_variant(&raw.mode).ok_or(StorageError::InvalidLegacyRecord {
        entity: "subscription",
        index,
        detail: "auto-download mode is malformed",
    })?;
    let mode = match name {
        "off" => AutoDownloadMode::Off,
        "allNew" => AutoDownloadMode::AllNew,
        "latestN" => {
            let count = payload.get("_0").and_then(Value::as_u64).ok_or(
                StorageError::InvalidLegacyRecord {
                    entity: "subscription",
                    index,
                    detail: "latest-N count is missing",
                },
            )?;
            AutoDownloadMode::Latest {
                count: u16::try_from(count).map_err(|_| StorageError::InvalidLegacyRecord {
                    entity: "subscription",
                    index,
                    detail: "latest-N count is outside supported range",
                })?,
            }
        }
        other => AutoDownloadMode::Unsupported {
            wire_code: unknown_wire_code(other),
        },
    };
    Ok(AutoDownloadPolicy {
        mode,
        wifi_only: raw.wifi_only,
    })
}
