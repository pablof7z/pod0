use pod0_domain::{ContentDigest, UnixTimestampMilliseconds};
use rusqlite::{Transaction, params};

use crate::feed_discovery_cutover_commit::commit_cutover;
use crate::feed_discovery_cutover_read::{read_report, read_staged_input};
use crate::feed_discovery_cutover_validation::validate_input;
use crate::{
    FeedDiscoveryCutoverState, LegacyFeedDiscoveryCutoverInput, LegacyFeedDiscoveryCutoverReport,
    LibraryStore, StorageError, feed_discovery_cutover_source_fingerprint,
    feed_discovery_cutover_source_generation,
};

pub fn inspect_legacy_feed_discovery_cutover(
    input: &LegacyFeedDiscoveryCutoverInput,
) -> Result<(ContentDigest, u64), StorageError> {
    validate_input(input)?;
    let fingerprint = feed_discovery_cutover_source_fingerprint(input);
    Ok((
        fingerprint,
        feed_discovery_cutover_source_generation(fingerprint),
    ))
}

impl LibraryStore {
    pub fn feed_discovery_cutover_report(
        &self,
    ) -> Result<LegacyFeedDiscoveryCutoverReport, StorageError> {
        self.read(read_report)
    }

    pub fn stage_legacy_feed_discovery_cutover(
        &self,
        input: LegacyFeedDiscoveryCutoverInput,
    ) -> Result<LegacyFeedDiscoveryCutoverReport, StorageError> {
        validate_input(&input)?;
        let fingerprint = feed_discovery_cutover_source_fingerprint(&input);
        let generation = feed_discovery_cutover_source_generation(fingerprint);
        self.write(|transaction| {
            let current = read_report(transaction)?;
            if current.state != FeedDiscoveryCutoverState::NotStarted {
                verify_existing(transaction, &input, fingerprint, generation, &current)?;
                return Ok(current);
            }
            ensure_target_ids_available(transaction, &input)?;
            stage_candidates(transaction, &input)?;
            transaction
                .execute(
                    "INSERT INTO pod0_feed_discovery_cutover(
                        singleton,state,source_generation,source_fingerprint,backup_digest,
                        backup_byte_count,notification_command_id,notifications_enabled,
                        inspected_job_count,candidate_count,blocked_count,ambiguous_count,
                        staged_at_ms,committed_at_ms
                     ) VALUES(1,'staged',?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,NULL)",
                    params![
                        to_i64(generation)?,
                        fingerprint.into_bytes().as_slice(),
                        input.backup_digest.into_bytes().as_slice(),
                        to_i64(input.backup_byte_count)?,
                        input.notification_command_id.into_bytes().as_slice(),
                        i64::from(input.notifications_enabled),
                        i64::from(input.inspected_job_count),
                        to_i64(input.candidates.len() as u64)?,
                        i64::from(input.blocked_count),
                        i64::from(input.ambiguous_count),
                        input.observed_at.value(),
                    ],
                )
                .map_err(|error| {
                    StorageError::sqlite("stage feed-discovery cutover evidence", error)
                })?;
            read_report(transaction)
        })
    }

    pub fn commit_legacy_feed_discovery_cutover(
        &self,
        source_generation: u64,
        observed_at: UnixTimestampMilliseconds,
    ) -> Result<LegacyFeedDiscoveryCutoverReport, StorageError> {
        self.write(|transaction| commit_cutover(transaction, source_generation, observed_at))
    }
}

fn stage_candidates(
    transaction: &Transaction<'_>,
    input: &LegacyFeedDiscoveryCutoverInput,
) -> Result<(), StorageError> {
    let mut candidates: Vec<_> = input.candidates.iter().collect();
    candidates.sort_by_key(|candidate| {
        (
            candidate.occurrence_id.into_bytes(),
            candidate.episode_id.into_bytes(),
            candidate.kind.wire(),
        )
    });
    for candidate in candidates {
        transaction
            .execute(
                "INSERT INTO pod0_feed_discovery_cutover_candidates(
                    occurrence_id,command_id,podcast_id,episode_id,kind,disposition,attempt,
                    not_before_ms,observed_at_ms,expires_at_ms,published_at_ms,input_version
                 ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
                params![
                    candidate.occurrence_id.into_bytes().as_slice(),
                    candidate.command_id.into_bytes().as_slice(),
                    candidate.podcast_id.into_bytes().as_slice(),
                    candidate.episode_id.into_bytes().as_slice(),
                    candidate.kind.wire(),
                    candidate.disposition.wire(),
                    i64::from(candidate.attempt),
                    candidate.not_before.map(|time| time.value()),
                    candidate.observed_at.value(),
                    candidate.expires_at.value(),
                    candidate.published_at.value(),
                    candidate.input_version,
                ],
            )
            .map_err(|error| {
                StorageError::sqlite("stage feed-discovery cutover candidate", error)
            })?;
    }
    Ok(())
}

fn ensure_target_ids_available(
    transaction: &Transaction<'_>,
    input: &LegacyFeedDiscoveryCutoverInput,
) -> Result<(), StorageError> {
    for candidate in &input.candidates {
        let episode_matches: i64 = transaction
            .query_row(
                "SELECT COUNT(*) FROM pod0_episodes WHERE episode_id=?1 AND podcast_id=?2",
                params![
                    candidate.episode_id.into_bytes().as_slice(),
                    candidate.podcast_id.into_bytes().as_slice()
                ],
                |row| row.get(0),
            )
            .map_err(|error| StorageError::sqlite("validate cutover episode", error))?;
        let identity_collision: i64 = transaction
            .query_row(
                "SELECT (
                    EXISTS(SELECT 1 FROM pod0_feed_discovery_occurrences
                           WHERE occurrence_id=?1 OR command_id=?2)
                    OR EXISTS(SELECT 1 FROM pod0_library_commands WHERE command_id=?2)
                 )",
                params![
                    candidate.occurrence_id.into_bytes().as_slice(),
                    candidate.command_id.into_bytes().as_slice()
                ],
                |row| row.get(0),
            )
            .map_err(|error| StorageError::sqlite("validate cutover identity", error))?;
        if episode_matches != 1 || identity_collision != 0 {
            return Err(StorageError::FeedDiscoveryCutoverConflict);
        }
    }
    Ok(())
}

fn verify_existing(
    transaction: &Transaction<'_>,
    input: &LegacyFeedDiscoveryCutoverInput,
    fingerprint: ContentDigest,
    generation: u64,
    report: &LegacyFeedDiscoveryCutoverReport,
) -> Result<(), StorageError> {
    if report.state.source_generation() != Some(generation)
        || report.source_fingerprint != Some(fingerprint)
        || report.backup_digest != Some(input.backup_digest)
        || report.backup_byte_count != Some(input.backup_byte_count)
        || report.notifications_enabled != Some(input.notifications_enabled)
        || report.inspected_job_count != input.inspected_job_count
        || report.candidate_count as usize != input.candidates.len()
        || report.blocked_count != input.blocked_count
        || report.ambiguous_count != input.ambiguous_count
    {
        return Err(StorageError::FeedDiscoveryCutoverConflict);
    }
    let staged = read_staged_input(transaction)?;
    if feed_discovery_cutover_source_fingerprint(&staged) != fingerprint {
        return Err(StorageError::FeedDiscoveryCutoverConflict);
    }
    Ok(())
}

pub(super) fn to_i64(value: u64) -> Result<i64, StorageError> {
    i64::try_from(value).map_err(|_| StorageError::InvalidFeedDiscoveryCutover)
}
