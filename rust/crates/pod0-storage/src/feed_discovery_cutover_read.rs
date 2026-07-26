use pod0_domain::{
    CommandId, ContentDigest, EpisodeId, FeedDiscoveryOccurrenceId, PodcastId,
    UnixTimestampMilliseconds,
};
use rusqlite::{Connection, OptionalExtension};

use crate::{
    FeedDiscoveryCutoverState, LegacyFeedDiscoveryCandidate, LegacyFeedDiscoveryCutoverInput,
    LegacyFeedDiscoveryCutoverReport, LegacyFeedDiscoveryDisposition,
    LegacyFeedDiscoveryEffectKind, StorageError,
};

type EvidenceRow = (
    String,
    i64,
    Vec<u8>,
    Vec<u8>,
    i64,
    Vec<u8>,
    i64,
    i64,
    i64,
    i64,
);

type StagedEvidenceRow = (Vec<u8>, i64, Vec<u8>, i64, i64, i64, i64, i64);

pub(super) fn read_report(
    connection: &Connection,
) -> Result<LegacyFeedDiscoveryCutoverReport, StorageError> {
    let row: Option<EvidenceRow> = connection
        .query_row(
            "SELECT state,source_generation,source_fingerprint,backup_digest,backup_byte_count,\
             notification_command_id,notifications_enabled,inspected_job_count,blocked_count,\
             ambiguous_count FROM pod0_feed_discovery_cutover WHERE singleton=1",
            [],
            |row| {
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
                ))
            },
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read feed-discovery cutover", error))?;
    let Some(row) = row else {
        return Ok(LegacyFeedDiscoveryCutoverReport {
            state: FeedDiscoveryCutoverState::NotStarted,
            source_fingerprint: None,
            backup_digest: None,
            backup_byte_count: None,
            notifications_enabled: None,
            inspected_job_count: 0,
            candidate_count: 0,
            blocked_count: 0,
            ambiguous_count: 0,
        });
    };
    let generation = positive_u64(row.1)?;
    let candidate_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM pod0_feed_discovery_cutover_candidates",
            [],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("count feed-discovery cutover candidates", error))?;
    Ok(LegacyFeedDiscoveryCutoverReport {
        state: match row.0.as_str() {
            "staged" => FeedDiscoveryCutoverState::Staged {
                source_generation: generation,
            },
            "authoritative" => FeedDiscoveryCutoverState::Authoritative {
                source_generation: generation,
            },
            _ => return Err(corrupt()),
        },
        source_fingerprint: Some(digest(row.2)?),
        backup_digest: Some(digest(row.3)?),
        backup_byte_count: Some(nonnegative_u64(row.4)?),
        notifications_enabled: Some(boolean(row.6)?),
        inspected_job_count: count(row.7)?,
        candidate_count: count(candidate_count)?,
        blocked_count: count(row.8)?,
        ambiguous_count: count(row.9)?,
    })
}

pub(super) fn read_staged_input(
    connection: &Connection,
) -> Result<LegacyFeedDiscoveryCutoverInput, StorageError> {
    let evidence: Option<StagedEvidenceRow> = connection
        .query_row(
            "SELECT backup_digest,backup_byte_count,notification_command_id,\
             notifications_enabled,inspected_job_count,blocked_count,ambiguous_count,staged_at_ms \
             FROM pod0_feed_discovery_cutover WHERE singleton=1",
            [],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                ))
            },
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read staged cutover evidence", error))?;
    let Some(evidence) = evidence else {
        return Err(StorageError::FeedDiscoveryCutoverConflict);
    };
    Ok(LegacyFeedDiscoveryCutoverInput {
        backup_digest: digest(evidence.0)?,
        backup_byte_count: nonnegative_u64(evidence.1)?,
        notification_command_id: id(evidence.2, CommandId::from_bytes)?,
        notifications_enabled: boolean(evidence.3)?,
        inspected_job_count: count(evidence.4)?,
        blocked_count: count(evidence.5)?,
        ambiguous_count: count(evidence.6)?,
        candidates: read_candidates(connection)?,
        observed_at: UnixTimestampMilliseconds::new(evidence.7),
    })
}

fn read_candidates(
    connection: &Connection,
) -> Result<Vec<LegacyFeedDiscoveryCandidate>, StorageError> {
    let mut statement = connection
        .prepare(
            "SELECT occurrence_id,command_id,podcast_id,episode_id,kind,disposition,attempt,\
             not_before_ms,observed_at_ms,expires_at_ms,published_at_ms,input_version \
             FROM pod0_feed_discovery_cutover_candidates \
             ORDER BY occurrence_id,episode_id,kind",
        )
        .map_err(|error| StorageError::sqlite("prepare cutover candidate read", error))?;
    let mut rows = statement
        .query([])
        .map_err(|error| StorageError::sqlite("query cutover candidates", error))?;
    let mut candidates = Vec::new();
    while let Some(row) = rows
        .next()
        .map_err(|error| StorageError::sqlite("iterate cutover candidates", error))?
    {
        let kind: String = row
            .get(4)
            .map_err(|error| StorageError::sqlite("decode cutover candidate", error))?;
        let disposition: String = row
            .get(5)
            .map_err(|error| StorageError::sqlite("decode cutover candidate", error))?;
        candidates.push(LegacyFeedDiscoveryCandidate {
            occurrence_id: id(
                row.get(0).map_err(sql)?,
                FeedDiscoveryOccurrenceId::from_bytes,
            )?,
            command_id: id(row.get(1).map_err(sql)?, CommandId::from_bytes)?,
            podcast_id: id(row.get(2).map_err(sql)?, PodcastId::from_bytes)?,
            episode_id: id(row.get(3).map_err(sql)?, EpisodeId::from_bytes)?,
            kind: decode_kind(&kind)?,
            disposition: decode_disposition(&disposition)?,
            attempt: u8::try_from(row.get::<_, i64>(6).map_err(sql)?).map_err(|_| corrupt())?,
            not_before: row
                .get::<_, Option<i64>>(7)
                .map_err(sql)?
                .map(UnixTimestampMilliseconds::new),
            observed_at: UnixTimestampMilliseconds::new(row.get(8).map_err(sql)?),
            expires_at: UnixTimestampMilliseconds::new(row.get(9).map_err(sql)?),
            published_at: UnixTimestampMilliseconds::new(row.get(10).map_err(sql)?),
            input_version: row.get(11).map_err(sql)?,
        });
    }
    Ok(candidates)
}

fn decode_kind(value: &str) -> Result<LegacyFeedDiscoveryEffectKind, StorageError> {
    match value {
        "download" => Ok(LegacyFeedDiscoveryEffectKind::Download),
        "notification" => Ok(LegacyFeedDiscoveryEffectKind::Notification),
        _ => Err(corrupt()),
    }
}

fn decode_disposition(value: &str) -> Result<LegacyFeedDiscoveryDisposition, StorageError> {
    match value {
        "pending" => Ok(LegacyFeedDiscoveryDisposition::Pending),
        "succeeded" => Ok(LegacyFeedDiscoveryDisposition::Succeeded),
        "obsolete" => Ok(LegacyFeedDiscoveryDisposition::Obsolete),
        "failed" => Ok(LegacyFeedDiscoveryDisposition::Failed),
        "ambiguous" => Ok(LegacyFeedDiscoveryDisposition::Ambiguous),
        _ => Err(corrupt()),
    }
}

fn id<T>(bytes: Vec<u8>, constructor: impl FnOnce([u8; 16]) -> T) -> Result<T, StorageError> {
    Ok(constructor(bytes.try_into().map_err(|_| corrupt())?))
}

fn digest(bytes: Vec<u8>) -> Result<ContentDigest, StorageError> {
    Ok(ContentDigest::from_bytes(
        bytes.try_into().map_err(|_| corrupt())?,
    ))
}

fn boolean(value: i64) -> Result<bool, StorageError> {
    match value {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(corrupt()),
    }
}

fn positive_u64(value: i64) -> Result<u64, StorageError> {
    let value = nonnegative_u64(value)?;
    (value > 0).then_some(value).ok_or_else(corrupt)
}

fn nonnegative_u64(value: i64) -> Result<u64, StorageError> {
    u64::try_from(value).map_err(|_| corrupt())
}

fn count(value: i64) -> Result<u32, StorageError> {
    u32::try_from(value).map_err(|_| corrupt())
}

fn sql(error: rusqlite::Error) -> StorageError {
    StorageError::sqlite("decode feed-discovery cutover candidate", error)
}

fn corrupt() -> StorageError {
    StorageError::CorruptSchema {
        detail: "feed-discovery cutover evidence is malformed",
    }
}
