use pod0_domain::{AutoDownloadPolicy, CommandId, PodcastId, StateRevision};
use rusqlite::{OptionalExtension, params};

use crate::StorageError;
use crate::library_store::{LibraryStore, command_was_applied, finish_command};
use crate::listening_db_codec::{auto_download, bool_value};

impl LibraryStore {
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
        self.write(|transaction| {
            if let Some(revision) =
                command_was_applied(transaction, command_id, command_fingerprint)?
            {
                return Ok(revision);
            }
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
            finish_command(transaction, command_id, command_fingerprint, observed_at_ms)
        })
    }

    pub fn unsubscribe(
        &self,
        command_id: CommandId,
        command_fingerprint: &str,
        podcast_id: PodcastId,
        observed_at_ms: i64,
    ) -> Result<StateRevision, StorageError> {
        self.write(|transaction| {
            if let Some(revision) =
                command_was_applied(transaction, command_id, command_fingerprint)?
            {
                return Ok(revision);
            }
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
                    "DELETE FROM pod0_episode_feed_metadata WHERE episode_id IN \
                 (SELECT episode_id FROM pod0_episodes WHERE podcast_id=?1)",
                    [podcast_id.into_bytes().as_slice()],
                )
                .map_err(|error| StorageError::sqlite("remove episode feed metadata", error))?;
            transaction
                .execute(
                    "DELETE FROM pod0_episodes WHERE podcast_id=?1",
                    [podcast_id.into_bytes().as_slice()],
                )
                .map_err(|error| StorageError::sqlite("remove podcast episodes", error))?;
            transaction
                .execute(
                    "DELETE FROM pod0_subscriptions WHERE podcast_id=?1",
                    [podcast_id.into_bytes().as_slice()],
                )
                .map_err(|error| StorageError::sqlite("remove subscription", error))?;
            transaction
                .execute(
                    "DELETE FROM pod0_podcasts WHERE podcast_id=?1",
                    [podcast_id.into_bytes().as_slice()],
                )
                .map_err(|error| StorageError::sqlite("remove podcast", error))?;
            finish_command(transaction, command_id, command_fingerprint, observed_at_ms)
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn update_subscription_preferences(
        &self,
        command_id: CommandId,
        command_fingerprint: &str,
        podcast_id: PodcastId,
        auto_download_policy: Option<AutoDownloadPolicy>,
        notifications_enabled: Option<bool>,
        observed_at_ms: i64,
    ) -> Result<StateRevision, StorageError> {
        self.write(|transaction| {
            if let Some(revision) =
                command_was_applied(transaction, command_id, command_fingerprint)?
            {
                return Ok(revision);
            }
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
            finish_command(transaction, command_id, command_fingerprint, observed_at_ms)
        })
    }
}
