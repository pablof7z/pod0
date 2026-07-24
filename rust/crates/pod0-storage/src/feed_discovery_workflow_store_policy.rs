impl LibraryStore {
    pub fn new_episode_notification_settings(
        &self,
    ) -> Result<NewEpisodeNotificationSettingsProjection, StorageError> {
        self.read(read_notification_settings)
    }

    pub fn set_new_episode_notifications_enabled(
        &self,
        command_id: CommandId,
        fingerprint: &str,
        enabled: bool,
        now_ms: i64,
    ) -> Result<NewEpisodeNotificationSettingsProjection, StorageError> {
        self.write(|transaction| {
            if command_was_applied(transaction, command_id, fingerprint)?.is_none() {
                let revision = finish_command(transaction, command_id, fingerprint, now_ms)?;
                let revision =
                    i64::try_from(revision.value).map_err(|_| StorageError::CorruptSchema {
                        detail: "new-episode notification revision overflows",
                    })?;
                transaction
                    .execute(
                        "UPDATE pod0_new_episode_notification_settings
                         SET enabled=?1,revision=?2,updated_at_ms=?3 WHERE singleton=1",
                        params![i64::from(enabled), revision, now_ms],
                    )
                    .map_err(|error| {
                        StorageError::sqlite("update new-episode notification setting", error)
                    })?;
            }
            read_notification_settings(transaction)
        })
    }
}

fn plan_pending(
    transaction: &Transaction<'_>,
    now_ms: i64,
    limit: i64,
) -> Result<usize, StorageError> {
    let settings = read_notification_settings(transaction)?;
    let occurrences = pending_occurrences(transaction, limit)?;
    let mut planned = 0;
    for (occurrence_id, podcast_id, is_initial, observed_at_ms) in occurrences {
        let expires_at_ms =
            observed_at_ms.saturating_add(FEED_DISCOVERY_NOTIFICATION_TTL_MILLISECONDS);
        let subscription = subscription_policy(transaction, podcast_id)?;
        let items = occurrence_items(transaction, occurrence_id)?;
        let mut effects = Vec::new();
        if let Some((mode, notifications_enabled)) = subscription {
            let download_count = match mode {
                AutoDownloadMode::Off | AutoDownloadMode::Unsupported { .. } => 0,
                AutoDownloadMode::Latest { count } => usize::from(count).min(items.len()),
                AutoDownloadMode::AllNew => items.len(),
            };
            effects.extend(
                items
                    .iter()
                    .take(download_count)
                    .map(|episode_id| (*episode_id, FeedDiscoveryEffectKind::Download)),
            );
            if !is_initial && settings.enabled && notifications_enabled && now_ms < expires_at_ms {
                effects.extend(
                    items
                        .iter()
                        .take(MAX_NEW_EPISODE_NOTIFICATIONS_PER_OCCURRENCE)
                        .map(|episode_id| (*episode_id, FeedDiscoveryEffectKind::Notification)),
                );
            }
        }
        let stage = if effects.is_empty() {
            "succeeded"
        } else {
            "active"
        };
        transaction
            .execute(
                "INSERT INTO pod0_feed_discovery_workflows(
                    occurrence_id,stage,workflow_revision,expires_at_ms,planned_at_ms,
                    completed_at_ms,updated_at_ms
                 ) VALUES(?1,?2,1,?3,?4,?5,?4)",
                params![
                    occurrence_id.into_bytes().as_slice(),
                    stage,
                    expires_at_ms,
                    now_ms,
                    (stage == "succeeded").then_some(now_ms)
                ],
            )
            .map_err(|error| StorageError::sqlite("plan feed-discovery workflow", error))?;
        for (episode_id, kind) in effects {
            let (command_id, cancellation_id) = match kind {
                FeedDiscoveryEffectKind::Download => (
                    Some(pod0_application::feed_discovery_download_command_id(
                        occurrence_id,
                        episode_id,
                    )),
                    pod0_application::feed_discovery_download_cancellation_id(
                        occurrence_id,
                        episode_id,
                    ),
                ),
                FeedDiscoveryEffectKind::Notification => (
                    None,
                    pod0_application::feed_discovery_notification_cancellation_id(
                        occurrence_id,
                        episode_id,
                    ),
                ),
            };
            transaction
                .execute(
                    "INSERT INTO pod0_feed_discovery_effects(
                        occurrence_id,episode_id,kind,stage,command_id,cancellation_id,
                        request_id,attempt,not_before_ms,deadline_at_ms,failure_code,
                        created_at_ms,updated_at_ms
                     ) VALUES(?1,?2,?3,'pending',?4,?5,NULL,0,NULL,NULL,NULL,?6,?6)",
                    params![
                        occurrence_id.into_bytes().as_slice(),
                        episode_id.into_bytes().as_slice(),
                        kind.wire(),
                        command_id.map(|value| value.into_bytes().to_vec()),
                        cancellation_id.into_bytes().as_slice(),
                        now_ms
                    ],
                )
                .map_err(|error| StorageError::sqlite("plan feed-discovery effect", error))?;
        }
        planned += 1;
    }
    Ok(planned)
}

fn read_notification_settings(
    connection: &Connection,
) -> Result<NewEpisodeNotificationSettingsProjection, StorageError> {
    connection
        .query_row(
            "SELECT enabled,revision FROM pod0_new_episode_notification_settings WHERE singleton=1",
            [],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
        )
        .map_err(|error| StorageError::sqlite("read new-episode notification setting", error))
        .and_then(|(enabled, revision)| {
            Ok(NewEpisodeNotificationSettingsProjection {
                enabled: boolean(enabled)?,
                revision: StateRevision::new(u64::try_from(revision).map_err(|_| {
                    StorageError::CorruptSchema {
                        detail: "new-episode notification revision is malformed",
                    }
                })?),
            })
        })
}

fn complete_workflows(transaction: &Transaction<'_>, now_ms: i64) -> Result<(), StorageError> {
    transaction
        .execute(
            "UPDATE pod0_feed_discovery_workflows AS workflow
             SET stage='succeeded',workflow_revision=workflow_revision+1,
                 completed_at_ms=?1,updated_at_ms=?1
             WHERE workflow.stage='active' AND NOT EXISTS(
               SELECT 1 FROM pod0_feed_discovery_effects effect
               WHERE effect.occurrence_id=workflow.occurrence_id
                 AND effect.stage IN ('pending','requested','retry_scheduled')
             )",
            [now_ms],
        )
        .map_err(|error| StorageError::sqlite("complete feed-discovery workflows", error))?;
    Ok(())
}

fn boolean(value: i64) -> Result<bool, StorageError> {
    match value {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(StorageError::CorruptSchema {
            detail: "feed-discovery boolean is malformed",
        }),
    }
}
