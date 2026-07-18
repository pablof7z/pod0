use std::collections::BTreeSet;

use crate::{
    AutoDownloadMode, CompletionStatus, DownloadArtifactStatus, EpisodeId, EpisodeIdentityRecord,
    EpisodeIdentityResolution, FeedIdentityV1, ListeningDomainError, ListeningDomainSnapshot,
    PlaybackRatePermille, PlaybackSleepMode, PodcastId, PodcastIdentityRecord,
    PodcastIdentityResolution, PodcastKind, TranscriptArtifactStatus,
};

pub const MIN_PLAYBACK_RATE_PERMILLE: u16 = 500;
pub const MAX_PLAYBACK_RATE_PERMILLE: u16 = 3_000;

fn feed_comparison_key_v1(feed_url: &str) -> Result<String, ListeningDomainError> {
    if feed_url.is_empty()
        || feed_url.chars().any(char::is_whitespace)
        || !(feed_url.to_ascii_lowercase().starts_with("https://")
            || feed_url.to_ascii_lowercase().starts_with("http://"))
    {
        return Err(ListeningDomainError::InvalidFeedUrl);
    }
    Ok(feed_url.to_lowercase())
}

#[uniffi::export]
pub fn make_feed_identity_v1(feed_url: String) -> Result<FeedIdentityV1, ListeningDomainError> {
    let comparison_key = feed_comparison_key_v1(&feed_url)?;
    Ok(FeedIdentityV1 {
        source_url: feed_url,
        comparison_key,
    })
}

/// Resolve modern-vs-legacy parent fields with the same precedence as Swift
/// Codable: the modern key wins when both are present.
#[uniffi::export]
pub fn resolve_legacy_parent_id(
    modern_parent_id: Option<PodcastId>,
    legacy_parent_id: Option<PodcastId>,
) -> Result<PodcastId, ListeningDomainError> {
    modern_parent_id
        .or(legacy_parent_id)
        .ok_or(ListeningDomainError::MissingLegacyParentIdentity)
}

#[uniffi::export]
pub fn resolve_podcast_identity_v1(
    incoming_id: PodcastId,
    incoming_feed_url: String,
    existing: Vec<PodcastIdentityRecord>,
) -> Result<PodcastIdentityResolution, ListeningDomainError> {
    let incoming_key = feed_comparison_key_v1(&incoming_feed_url)?;
    let mut matching_ids = BTreeSet::new();
    let mut incoming_id_key = None;
    for candidate in existing {
        let expected = feed_comparison_key_v1(&candidate.feed_identity.source_url)?;
        if candidate.feed_identity.comparison_key != expected {
            return Err(ListeningDomainError::InvalidFeedComparisonKey);
        }
        if candidate.feed_identity.comparison_key == incoming_key {
            matching_ids.insert(candidate.podcast_id);
        }
        if candidate.podcast_id == incoming_id {
            incoming_id_key = Some(candidate.feed_identity.comparison_key);
        }
    }
    if matching_ids.len() > 1 {
        return Err(ListeningDomainError::AmbiguousPodcastFeedIdentity);
    }
    if let Some(existing_key) = incoming_id_key
        && existing_key != incoming_key
    {
        return Err(ListeningDomainError::PodcastIdConflict);
    }
    if let Some(podcast_id) = matching_ids.into_iter().next() {
        Ok(PodcastIdentityResolution::PreserveExisting { podcast_id })
    } else {
        Ok(PodcastIdentityResolution::AcceptIncoming {
            podcast_id: incoming_id,
        })
    }
}

#[uniffi::export]
pub fn resolve_episode_identity_v1(
    incoming_id: EpisodeId,
    podcast_id: PodcastId,
    publisher_guid: String,
    existing: Vec<EpisodeIdentityRecord>,
) -> Result<EpisodeIdentityResolution, ListeningDomainError> {
    if publisher_guid.is_empty() {
        return Err(ListeningDomainError::EmptyPublisherGuid);
    }
    let mut matching_ids = BTreeSet::new();
    let mut incoming_id_identity = None;
    for candidate in existing {
        if candidate.publisher_guid.is_empty() {
            return Err(ListeningDomainError::EmptyPublisherGuid);
        }
        if candidate.podcast_id == podcast_id && candidate.publisher_guid == publisher_guid {
            matching_ids.insert(candidate.episode_id);
        }
        if candidate.episode_id == incoming_id {
            incoming_id_identity = Some((candidate.podcast_id, candidate.publisher_guid));
        }
    }
    if matching_ids.len() > 1 {
        return Err(ListeningDomainError::AmbiguousEpisodeIdentity);
    }
    if let Some((existing_parent, existing_guid)) = incoming_id_identity
        && (existing_parent != podcast_id || existing_guid != publisher_guid)
    {
        return Err(ListeningDomainError::EpisodeIdConflict);
    }
    if let Some(episode_id) = matching_ids.into_iter().next() {
        Ok(EpisodeIdentityResolution::PreserveExisting { episode_id })
    } else {
        Ok(EpisodeIdentityResolution::AcceptIncoming {
            episode_id: incoming_id,
        })
    }
}

fn validate_rate(rate: PlaybackRatePermille) -> Result<(), ListeningDomainError> {
    if (MIN_PLAYBACK_RATE_PERMILLE..=MAX_PLAYBACK_RATE_PERMILLE).contains(&rate.value) {
        Ok(())
    } else {
        Err(ListeningDomainError::InvalidPlaybackRate)
    }
}

fn validate_artifact(version: u32, key: &str) -> Result<(), ListeningDomainError> {
    if version == 0 || key.is_empty() {
        Err(ListeningDomainError::InvalidArtifactReference)
    } else {
        Ok(())
    }
}

/// Typed import/preflight boundary used by native migration adapters. It never
/// generates identities or mutates durable state.
#[uniffi::export]
pub fn validate_listening_snapshot(
    snapshot: ListeningDomainSnapshot,
) -> Result<ListeningDomainSnapshot, ListeningDomainError> {
    let mut podcast_ids = BTreeSet::new();
    let mut feed_identities = BTreeSet::new();
    for podcast in &snapshot.podcasts {
        if !podcast_ids.insert(podcast.podcast_id) {
            return Err(ListeningDomainError::DuplicatePodcastId);
        }
        match (&podcast.kind, &podcast.feed_identity) {
            (PodcastKind::Rss, Some(feed)) => {
                if feed_comparison_key_v1(&feed.source_url)? != feed.comparison_key {
                    return Err(ListeningDomainError::InvalidFeedComparisonKey);
                }
                if !feed_identities.insert(feed.comparison_key.clone()) {
                    return Err(ListeningDomainError::AmbiguousPodcastFeedIdentity);
                }
            }
            (PodcastKind::Rss, None) => return Err(ListeningDomainError::InvalidFeedUrl),
            _ => {}
        }
    }

    let mut subscription_ids = BTreeSet::new();
    for subscription in &snapshot.subscriptions {
        if !podcast_ids.contains(&subscription.podcast_id) {
            return Err(ListeningDomainError::MissingPodcast);
        }
        if !subscription_ids.insert(subscription.podcast_id) {
            return Err(ListeningDomainError::DuplicateSubscription);
        }
        if matches!(
            subscription.auto_download.mode,
            AutoDownloadMode::Latest { count: 0 }
        ) {
            return Err(ListeningDomainError::InvalidLatestCount);
        }
        if let Some(rate) = subscription.default_playback_rate {
            validate_rate(rate)?;
        }
    }

    let mut episode_ids = BTreeSet::new();
    let mut episode_identities = BTreeSet::new();
    for episode in &snapshot.episodes {
        if !episode_ids.insert(episode.episode_id) {
            return Err(ListeningDomainError::DuplicateEpisodeId);
        }
        if !podcast_ids.contains(&episode.podcast_id) {
            return Err(ListeningDomainError::MissingPodcast);
        }
        if episode.publisher_guid.is_empty() {
            return Err(ListeningDomainError::EmptyPublisherGuid);
        }
        if !episode_identities.insert((episode.podcast_id, episode.publisher_guid.clone())) {
            return Err(ListeningDomainError::AmbiguousEpisodeIdentity);
        }
        if matches!(
            episode.listening.completion,
            CompletionStatus::Completed { cause }
                if !matches!(cause, crate::CompletionCause::LegacyPlayedFlag)
        ) && episode.listening.resume_position_milliseconds != 0
        {
            return Err(ListeningDomainError::CompletedEpisodeHasResumePosition);
        }
        match &episode.download {
            DownloadArtifactStatus::Available { reference, .. } => {
                validate_artifact(reference.schema_version, &reference.opaque_key)?;
            }
            DownloadArtifactStatus::Unavailable | DownloadArtifactStatus::Unsupported { .. } => {}
        }
        match &episode.transcript {
            TranscriptArtifactStatus::Available { reference, .. } => {
                validate_artifact(reference.schema_version, &reference.opaque_key)?;
            }
            TranscriptArtifactStatus::Unavailable
            | TranscriptArtifactStatus::Unsupported { .. } => {}
        }
    }

    let mut queue_ids = BTreeSet::new();
    for entry in &snapshot.playback.queue {
        if !queue_ids.insert(entry.queue_entry_id) {
            return Err(ListeningDomainError::DuplicateQueueEntryId);
        }
        if !episode_ids.contains(&entry.episode_id) {
            return Err(ListeningDomainError::MissingEpisode);
        }
        if let Some(segment) = entry.segment
            && let Some(end) = segment.end_position_milliseconds
            && end <= segment.start_position_milliseconds.unwrap_or(0)
        {
            return Err(ListeningDomainError::InvalidSegmentBounds);
        }
    }
    if snapshot
        .playback
        .active_episode_id
        .is_some_and(|episode_id| !episode_ids.contains(&episode_id))
    {
        return Err(ListeningDomainError::MissingEpisode);
    }
    validate_rate(snapshot.playback.rate)?;
    if matches!(
        snapshot.playback.sleep_mode,
        PlaybackSleepMode::Duration {
            duration_milliseconds: 0
        }
    ) {
        return Err(ListeningDomainError::InvalidSleepDuration);
    }
    Ok(snapshot)
}
