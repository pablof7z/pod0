type RawEffectRow = (
    Vec<u8>,
    Vec<u8>,
    Vec<u8>,
    String,
    String,
    String,
    String,
    Vec<u8>,
    Vec<u8>,
    Option<Vec<u8>>,
    i64,
    Option<i64>,
    Option<i64>,
    i64,
    i64,
);

fn decode_effect_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<RawEffectRow> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
        row.get(6)?,
        row.get(7)?,
        row.get(8)?,
        row.get(9)?,
        row.get(10)?,
        row.get(11)?,
        row.get(12)?,
        row.get(13)?,
        row.get(14)?,
    ))
}

fn decode_effect(row: RawEffectRow) -> Result<FeedDiscoveryEffectRecord, StorageError> {
    let (
        occurrence,
        podcast,
        episode,
        podcast_title,
        episode_title,
        kind,
        stage,
        command,
        cancellation,
        request,
        attempt,
        not_before,
        deadline,
        expires,
        revision,
    ) = row;
    Ok(FeedDiscoveryEffectRecord {
        occurrence_id: decode_workflow_id(occurrence, FeedDiscoveryOccurrenceId::from_bytes)?,
        podcast_id: decode_workflow_id(podcast, PodcastId::from_bytes)?,
        episode_id: decode_workflow_id(episode, EpisodeId::from_bytes)?,
        podcast_title,
        episode_title,
        kind: decode_kind(&kind)?,
        stage: decode_stage(&stage)?,
        command_id: Some(decode_workflow_id(command, CommandId::from_bytes)?),
        cancellation_id: decode_workflow_id(cancellation, CancellationId::from_bytes)?,
        request_id: request
            .map(|value| decode_workflow_id(value, HostRequestId::from_bytes))
            .transpose()?,
        attempt: u8::try_from(attempt).map_err(|_| StorageError::CorruptSchema {
            detail: "feed-discovery effect attempt is malformed",
        })?,
        not_before_ms: not_before,
        deadline_at_ms: deadline,
        expires_at_ms: expires,
        workflow_revision: StateRevision::new(u64::try_from(revision).map_err(|_| {
            StorageError::CorruptSchema {
                detail: "feed-discovery workflow revision is malformed",
            }
        })?),
    })
}

fn decode_kind(value: &str) -> Result<FeedDiscoveryEffectKind, StorageError> {
    match value {
        "download" => Ok(FeedDiscoveryEffectKind::Download),
        "notification" => Ok(FeedDiscoveryEffectKind::Notification),
        _ => Err(StorageError::CorruptSchema {
            detail: "feed-discovery effect kind is malformed",
        }),
    }
}

fn decode_stage(value: &str) -> Result<FeedDiscoveryEffectStage, StorageError> {
    match value {
        "pending" => Ok(FeedDiscoveryEffectStage::Pending),
        "requested" => Ok(FeedDiscoveryEffectStage::Requested),
        "retry_scheduled" => Ok(FeedDiscoveryEffectStage::RetryScheduled),
        "succeeded" => Ok(FeedDiscoveryEffectStage::Succeeded),
        "obsolete" => Ok(FeedDiscoveryEffectStage::Obsolete),
        "failed" => Ok(FeedDiscoveryEffectStage::Failed),
        _ => Err(StorageError::CorruptSchema {
            detail: "feed-discovery effect stage is malformed",
        }),
    }
}

fn decode_workflow_id<T>(
    value: Vec<u8>,
    constructor: impl FnOnce([u8; 16]) -> T,
) -> Result<T, StorageError> {
    Ok(constructor(value.try_into().map_err(|_| {
        StorageError::CorruptSchema {
            detail: "feed-discovery workflow identity is malformed",
        }
    })?))
}
