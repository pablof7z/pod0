fn pending_occurrences(
    connection: &Connection,
    limit: i64,
) -> Result<Vec<(FeedDiscoveryOccurrenceId, PodcastId, bool, i64)>, StorageError> {
    let mut statement = connection
        .prepare(
            "SELECT occurrence.occurrence_id,occurrence.podcast_id,
                    occurrence.is_initial_population,occurrence.observed_at_ms
             FROM pod0_feed_discovery_occurrences occurrence
             LEFT JOIN pod0_feed_discovery_workflows workflow
               ON workflow.occurrence_id=occurrence.occurrence_id
             WHERE workflow.occurrence_id IS NULL
             ORDER BY occurrence.observed_at_ms,occurrence.occurrence_id LIMIT ?1",
        )
        .map_err(|error| StorageError::sqlite("prepare unplanned feed discoveries", error))?;
    let rows = statement
        .query_map([limit], |row| {
            Ok((
                row.get::<_, Vec<u8>>(0)?,
                row.get::<_, Vec<u8>>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
            ))
        })
        .map_err(|error| StorageError::sqlite("query unplanned feed discoveries", error))?;
    rows.map(|row| {
        let (occurrence, podcast, initial, observed_at) =
            row.map_err(|error| StorageError::sqlite("read unplanned feed discovery", error))?;
        Ok((
            decode_workflow_id(occurrence, FeedDiscoveryOccurrenceId::from_bytes)?,
            decode_workflow_id(podcast, PodcastId::from_bytes)?,
            boolean(initial)?,
            observed_at,
        ))
    })
    .collect()
}

fn subscription_policy(
    connection: &Connection,
    podcast_id: PodcastId,
) -> Result<Option<(AutoDownloadMode, bool)>, StorageError> {
    let row: Option<(i64, Option<i64>, Option<i64>, i64)> = connection
        .query_row(
            "SELECT auto_download_code,auto_download_wire_code,auto_download_latest_count,
                    notifications_enabled
             FROM pod0_subscriptions WHERE podcast_id=?1",
            [podcast_id.into_bytes().as_slice()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read feed-discovery preferences", error))?;
    row.map(|(code, wire, latest, notifications)| {
        Ok((
            decode_auto_download(code, wire, latest)?,
            boolean(notifications)?,
        ))
    })
    .transpose()
}

fn occurrence_items(
    connection: &Connection,
    occurrence_id: FeedDiscoveryOccurrenceId,
) -> Result<Vec<EpisodeId>, StorageError> {
    let mut statement = connection
        .prepare(
            "SELECT episode_id FROM pod0_feed_discovery_items
             WHERE occurrence_id=?1 ORDER BY published_at_ms DESC,episode_id",
        )
        .map_err(|error| StorageError::sqlite("prepare feed-discovery policy items", error))?;
    let rows = statement
        .query_map([occurrence_id.into_bytes().as_slice()], |row| {
            row.get::<_, Vec<u8>>(0)
        })
        .map_err(|error| StorageError::sqlite("query feed-discovery policy items", error))?;
    rows.map(|row| {
        decode_workflow_id(
            row.map_err(|error| StorageError::sqlite("read feed-discovery policy item", error))?,
            EpisodeId::from_bytes,
        )
    })
    .collect()
}

fn read_effects(
    connection: &Connection,
    kind: FeedDiscoveryEffectKind,
    now_ms: i64,
    limit: i64,
) -> Result<Vec<FeedDiscoveryEffectRecord>, StorageError> {
    let mut statement = connection
        .prepare(
            "SELECT effect.occurrence_id,occurrence.podcast_id,effect.episode_id,
                    podcast.title,episode.title,effect.kind,effect.stage,
                    COALESCE(effect.command_id,occurrence.command_id),effect.cancellation_id,
                    effect.request_id,effect.attempt,effect.not_before_ms,effect.deadline_at_ms,
                    workflow.expires_at_ms,workflow.workflow_revision
             FROM pod0_feed_discovery_effects effect
             JOIN pod0_feed_discovery_occurrences occurrence
               ON occurrence.occurrence_id=effect.occurrence_id
             JOIN pod0_feed_discovery_workflows workflow
               ON workflow.occurrence_id=effect.occurrence_id
             JOIN pod0_podcasts podcast ON podcast.podcast_id=occurrence.podcast_id
             JOIN pod0_episodes episode ON episode.episode_id=effect.episode_id
             WHERE effect.kind=?1
               AND effect.stage IN ('pending','retry_scheduled')
               AND COALESCE(effect.not_before_ms,0)<=?2
             ORDER BY workflow.planned_at_ms,effect.occurrence_id,
                      episode.published_at_ms DESC,effect.episode_id LIMIT ?3",
        )
        .map_err(|error| StorageError::sqlite("prepare feed-discovery effects", error))?;
    let rows = statement
        .query_map(params![kind.wire(), now_ms, limit], decode_effect_row)
        .map_err(|error| StorageError::sqlite("query feed-discovery effects", error))?;
    rows.map(|row| {
        row.map_err(|error| StorageError::sqlite("read feed-discovery effect", error))
            .and_then(decode_effect)
    })
    .collect()
}

fn read_requested_notification_effects(
    connection: &Connection,
    limit: i64,
) -> Result<Vec<FeedDiscoveryEffectRecord>, StorageError> {
    let mut statement = connection
        .prepare(
            "SELECT effect.occurrence_id,occurrence.podcast_id,effect.episode_id,
                    podcast.title,episode.title,effect.kind,effect.stage,
                    COALESCE(effect.command_id,occurrence.command_id),effect.cancellation_id,
                    effect.request_id,effect.attempt,effect.not_before_ms,effect.deadline_at_ms,
                    workflow.expires_at_ms,workflow.workflow_revision
             FROM pod0_feed_discovery_effects effect
             JOIN pod0_feed_discovery_occurrences occurrence
               ON occurrence.occurrence_id=effect.occurrence_id
             JOIN pod0_feed_discovery_workflows workflow
               ON workflow.occurrence_id=effect.occurrence_id
             JOIN pod0_podcasts podcast ON podcast.podcast_id=occurrence.podcast_id
             JOIN pod0_episodes episode ON episode.episode_id=effect.episode_id
             WHERE effect.kind='notification' AND effect.stage='requested'
             ORDER BY effect.updated_at_ms,effect.request_id LIMIT ?1",
        )
        .map_err(|error| StorageError::sqlite("prepare requested notifications", error))?;
    let rows = statement
        .query_map([limit], decode_effect_row)
        .map_err(|error| StorageError::sqlite("query requested notifications", error))?;
    rows.map(|row| {
        row.map_err(|error| StorageError::sqlite("read requested notification", error))
            .and_then(decode_effect)
    })
    .collect()
}

fn read_effect(
    connection: &Connection,
    occurrence_id: FeedDiscoveryOccurrenceId,
    episode_id: EpisodeId,
    kind: FeedDiscoveryEffectKind,
) -> Result<Option<FeedDiscoveryEffectRecord>, StorageError> {
    connection
        .query_row(
            "SELECT effect.occurrence_id,occurrence.podcast_id,effect.episode_id,
                    podcast.title,episode.title,effect.kind,effect.stage,
                    COALESCE(effect.command_id,occurrence.command_id),effect.cancellation_id,
                    effect.request_id,effect.attempt,effect.not_before_ms,effect.deadline_at_ms,
                    workflow.expires_at_ms,workflow.workflow_revision
             FROM pod0_feed_discovery_effects effect
             JOIN pod0_feed_discovery_occurrences occurrence
               ON occurrence.occurrence_id=effect.occurrence_id
             JOIN pod0_feed_discovery_workflows workflow
               ON workflow.occurrence_id=effect.occurrence_id
             JOIN pod0_podcasts podcast ON podcast.podcast_id=occurrence.podcast_id
             JOIN pod0_episodes episode ON episode.episode_id=effect.episode_id
             WHERE effect.occurrence_id=?1 AND effect.episode_id=?2 AND effect.kind=?3",
            params![
                occurrence_id.into_bytes().as_slice(),
                episode_id.into_bytes().as_slice(),
                kind.wire()
            ],
            decode_effect_row,
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read one feed-discovery effect", error))?
        .map(decode_effect)
        .transpose()
}

fn effect_for_request(
    connection: &Connection,
    request_id: HostRequestId,
) -> Result<Option<FeedDiscoveryEffectRecord>, StorageError> {
    let identity: Option<(Vec<u8>, Vec<u8>)> = connection
        .query_row(
            "SELECT occurrence_id,episode_id FROM pod0_feed_discovery_effects
             WHERE request_id=?1 AND kind='notification'",
            [request_id.into_bytes().as_slice()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|error| StorageError::sqlite("find notification request", error))?;
    let Some((occurrence, episode)) = identity else {
        return Ok(None);
    };
    read_effect(
        connection,
        decode_workflow_id(occurrence, FeedDiscoveryOccurrenceId::from_bytes)?,
        decode_workflow_id(episode, EpisodeId::from_bytes)?,
        FeedDiscoveryEffectKind::Notification,
    )
}

fn requested_notification_ids(
    connection: &Connection,
    now_ms: i64,
) -> Result<Vec<HostRequestId>, StorageError> {
    let mut statement = connection
        .prepare(
            "SELECT request_id FROM pod0_feed_discovery_effects
             WHERE kind='notification' AND stage='requested'
               AND deadline_at_ms<=?1 AND request_id IS NOT NULL
             ORDER BY deadline_at_ms,request_id LIMIT 64",
        )
        .map_err(|error| StorageError::sqlite("prepare expired notifications", error))?;
    let rows = statement
        .query_map([now_ms], |row| row.get::<_, Vec<u8>>(0))
        .map_err(|error| StorageError::sqlite("query expired notifications", error))?;
    rows.map(|row| {
        decode_workflow_id(
            row.map_err(|error| StorageError::sqlite("read expired notification", error))?,
            HostRequestId::from_bytes,
        )
    })
    .collect()
}

fn finish_notification_timeout(
    transaction: &Transaction<'_>,
    request_id: HostRequestId,
    now_ms: i64,
) -> Result<bool, StorageError> {
    let Some(record) = effect_for_request(transaction, request_id)? else {
        return Ok(false);
    };
    let retry = record.attempt < FEED_DISCOVERY_NOTIFICATION_MAX_ATTEMPTS
        && now_ms < record.expires_at_ms;
    let (stage, not_before) = if retry {
        (
            "retry_scheduled",
            Some(now_ms.saturating_add(FEED_DISCOVERY_NOTIFICATION_RETRY_MILLISECONDS)),
        )
    } else {
        ("failed", None)
    };
    transaction
        .execute(
            "UPDATE pod0_feed_discovery_effects
             SET stage=?1,request_id=NULL,deadline_at_ms=NULL,not_before_ms=?2,
                 failure_code='timed_out',updated_at_ms=?3
             WHERE request_id=?4 AND stage='requested'",
            params![
                stage,
                not_before,
                now_ms,
                request_id.into_bytes().as_slice()
            ],
        )
        .map(|changed| changed == 1)
        .map_err(|error| StorageError::sqlite("expire notification request", error))
}
