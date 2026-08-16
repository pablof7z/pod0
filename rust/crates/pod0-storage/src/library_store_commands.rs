use pod0_domain::{
    AutoDownloadPolicy, CommandId, EpisodeId, PodcastId, StateRevision, TranscriptStartPolicy,
};
use rusqlite::{OptionalExtension, params};

use crate::StorageError;
use crate::library_store::LibraryStore;
use crate::library_store_clip_support::set_clip_revision;
use crate::listening_db_codec::{auto_download, bool_value, transcript_start_policy};
use pod0_application::{ActivitySubject, LibraryFeedTransition};

impl LibraryStore {
    pub fn set_episode_starred(
        &self,
        command_id: CommandId,
        command_fingerprint: &str,
        episode_id: EpisodeId,
        starred: bool,
        observed_at_ms: i64,
    ) -> Result<StateRevision, StorageError> {
        crate::transition_commit::commit_episode_starred(
            self.path(),
            command_id,
            command_fingerprint,
            episode_id,
            starred,
            observed_at_ms,
        )
    }

    pub fn reset_listening_data(
        &self,
        command_id: CommandId,
        command_fingerprint: &str,
        observed_at_ms: i64,
    ) -> Result<StateRevision, StorageError> {
        self.reset_listening_data_with_effects(
            command_id,
            command_fingerprint,
            Vec::new(),
            observed_at_ms,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn mark_feed_not_modified(
        &self,
        command_id: CommandId,
        command_fingerprint: &str,
        podcast_id: PodcastId,
        entity_tag: Option<String>,
        last_modified: Option<String>,
        observed_at_ms: i64,
    ) -> Result<StateRevision, StorageError> {
        self.commit_library_activity(
            command_id,
            command_fingerprint,
            ActivitySubject::Podcast { podcast_id },
            None,
            LibraryFeedTransition::FeedFetchStateChanged,
            observed_at_ms,
            |_| Ok((true, (entity_tag, last_modified))),
            |transaction, (entity_tag, last_modified)| {
                let changed = transaction
                    .execute(
                        "UPDATE pod0_podcasts SET last_refreshed_at_ms=?1,\
                 etag=COALESCE(?2,etag),last_modified=COALESCE(?3,last_modified) \
                 WHERE podcast_id=?4",
                        params![
                            observed_at_ms,
                            entity_tag,
                            last_modified,
                            podcast_id.into_bytes().as_slice()
                        ],
                    )
                    .map_err(|error| StorageError::sqlite("record not-modified feed", error))?;
                if changed != 1 {
                    return Err(StorageError::EntityNotFound);
                }
                Ok(())
            },
            |_, _| Ok(()),
        )
    }

    pub fn unsubscribe(
        &self,
        command_id: CommandId,
        command_fingerprint: &str,
        podcast_id: PodcastId,
        observed_at_ms: i64,
    ) -> Result<StateRevision, StorageError> {
        self.commit_library_activity(
            command_id,
            command_fingerprint,
            ActivitySubject::Podcast { podcast_id },
            None,
            LibraryFeedTransition::SubscriptionChanged,
            observed_at_ms,
            |transaction| {
                let exists: Option<i64> = transaction
                    .query_row(
                        "SELECT 1 FROM pod0_podcasts WHERE podcast_id=?1",
                        [podcast_id.into_bytes().as_slice()],
                        |row| row.get(0),
                    )
                    .optional()
                    .map_err(|error| StorageError::sqlite("find podcast for removal", error))?;
                if exists.is_none() {
                    return Err(StorageError::EntityNotFound);
                }
                Ok((true, ()))
            },
            |transaction, ()| {
                transaction
                    .execute(
                        "DELETE FROM pod0_queue_entries WHERE episode_id IN \
                 (SELECT episode_id FROM pod0_episodes WHERE podcast_id=?1)",
                        [podcast_id.into_bytes().as_slice()],
                    )
                    .map_err(|error| StorageError::sqlite("remove podcast queue entries", error))?;
                transaction.execute(
                "UPDATE pod0_playback_state SET active_episode_id=NULL WHERE active_episode_id IN \
                 (SELECT episode_id FROM pod0_episodes WHERE podcast_id=?1)",
                [podcast_id.into_bytes().as_slice()],
            ).map_err(|error| StorageError::sqlite("clear removed active episode", error))?;
                transaction
                    .execute(
                        "DELETE FROM pod0_subscriptions WHERE podcast_id=?1",
                        [podcast_id.into_bytes().as_slice()],
                    )
                    .map_err(|error| StorageError::sqlite("remove subscription", error))?;
                transaction
                    .execute(
                        "UPDATE pod0_podcasts SET library_visible=0 WHERE podcast_id=?1",
                        [podcast_id.into_bytes().as_slice()],
                    )
                    .map_err(|error| StorageError::sqlite("hide unsubscribed podcast", error))?;
                Ok(())
            },
            set_clip_revision,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn update_subscription_preferences(
        &self,
        command_id: CommandId,
        command_fingerprint: &str,
        podcast_id: PodcastId,
        auto_download_policy: Option<AutoDownloadPolicy>,
        notifications_enabled: Option<bool>,
        transcript_policy: Option<TranscriptStartPolicy>,
        observed_at_ms: i64,
    ) -> Result<StateRevision, StorageError> {
        let transition = if transcript_policy.is_some() {
            LibraryFeedTransition::TranscriptPreferenceChanged
        } else if notifications_enabled.is_some() {
            LibraryFeedTransition::NotificationPreferenceChanged
        } else {
            LibraryFeedTransition::SubscriptionChanged
        };
        self.commit_library_activity(
            command_id,
            command_fingerprint,
            ActivitySubject::Podcast { podcast_id },
            None,
            transition,
            observed_at_ms,
            |transaction| {
                let exists: Option<i64> = transaction
                    .query_row(
                        "SELECT 1 FROM pod0_subscriptions WHERE podcast_id=?1",
                        [podcast_id.into_bytes().as_slice()],
                        |row| row.get(0),
                    )
                    .optional()
                    .map_err(|error| {
                        StorageError::sqlite("find subscription for preference", error)
                    })?;
                if exists.is_none() {
                    return Err(StorageError::EntityNotFound);
                }
                Ok((
                    true,
                    (
                        auto_download_policy,
                        notifications_enabled,
                        transcript_policy,
                    ),
                ))
            },
            |transaction, (auto_download_policy, notifications_enabled, transcript_policy)| {
                if let Some(policy) = auto_download_policy {
                    let (code, wire, latest) = auto_download(&policy.mode);
                    let changed = transaction
                        .execute(
                            "UPDATE pod0_subscriptions SET auto_download_code=?1,\
                     auto_download_wire_code=?2,auto_download_latest_count=?3,wifi_only=?4 \
                     WHERE podcast_id=?5",
                            params![
                                code,
                                wire,
                                latest,
                                bool_value(policy.wifi_only),
                                podcast_id.into_bytes().as_slice()
                            ],
                        )
                        .map_err(|error| {
                            StorageError::sqlite("update auto-download preference", error)
                        })?;
                    if changed != 1 {
                        return Err(StorageError::EntityNotFound);
                    }
                }
                if let Some(enabled) = notifications_enabled {
                    let changed = transaction.execute(
                    "UPDATE pod0_subscriptions SET notifications_enabled=?1 WHERE podcast_id=?2",
                    params![bool_value(enabled), podcast_id.into_bytes().as_slice()],
                ).map_err(|error| StorageError::sqlite("update notification preference", error))?;
                    if changed != 1 {
                        return Err(StorageError::EntityNotFound);
                    }
                }
                if let Some(policy) = transcript_policy {
                    let (code, wire) = transcript_start_policy(&policy);
                    let changed = transaction
                        .execute(
                            "UPDATE pod0_subscriptions SET transcript_start_policy_code=?1,\
                     transcript_start_policy_wire_code=?2 WHERE podcast_id=?3",
                            params![code, wire, podcast_id.into_bytes().as_slice()],
                        )
                        .map_err(|error| {
                            StorageError::sqlite("update transcript start preference", error)
                        })?;
                    if changed != 1 {
                        return Err(StorageError::EntityNotFound);
                    }
                }
                Ok(())
            },
            |_, _| Ok(()),
        )
    }
}
