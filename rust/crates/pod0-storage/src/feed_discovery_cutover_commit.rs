use std::collections::BTreeMap;

use pod0_domain::{EpisodeId, FeedDiscoveryOccurrenceId, UnixTimestampMilliseconds};
use rusqlite::{Transaction, params};
use sha2::{Digest as _, Sha256};

use crate::feed_discovery_cutover::to_i64;
use crate::feed_discovery_cutover_read::{read_report, read_staged_input};
use crate::feed_discovery_cutover_validation::validate_input;
use crate::library_store::command_was_applied;
use crate::{
    FeedDiscoveryCutoverState, LegacyFeedDiscoveryCandidate, LegacyFeedDiscoveryCutoverReport,
    LegacyFeedDiscoveryDisposition, LegacyFeedDiscoveryEffectKind, StorageError,
    feed_discovery_cutover_source_fingerprint,
};

pub(super) fn commit_cutover(
    transaction: &Transaction<'_>,
    source_generation: u64,
    observed_at: UnixTimestampMilliseconds,
    committed_revision: pod0_domain::StateRevision,
) -> Result<LegacyFeedDiscoveryCutoverReport, StorageError> {
    let report = read_report(transaction)?;
    match report.state {
        FeedDiscoveryCutoverState::Authoritative {
            source_generation: existing,
        } if existing == source_generation => return Ok(report),
        FeedDiscoveryCutoverState::Staged {
            source_generation: existing,
        } if existing == source_generation => {}
        _ => return Err(StorageError::FeedDiscoveryCutoverConflict),
    }
    let input = read_staged_input(transaction)?;
    validate_input(&input)?;
    if report.source_fingerprint != Some(feed_discovery_cutover_source_fingerprint(&input)) {
        return Err(StorageError::FeedDiscoveryCutoverConflict);
    }
    import_notification_setting(transaction, &input, observed_at.value(), committed_revision)?;
    import_occurrences(
        transaction,
        &input.candidates,
        observed_at.value(),
        committed_revision,
    )?;
    transaction
        .execute(
            "UPDATE pod0_feed_discovery_cutover
             SET state='authoritative',committed_at_ms=?1
             WHERE singleton=1 AND state='staged' AND source_generation=?2",
            params![observed_at.value(), to_i64(source_generation)?],
        )
        .map_err(|error| StorageError::sqlite("commit feed-discovery cutover", error))?;
    if transaction.changes() != 1 {
        return Err(StorageError::FeedDiscoveryCutoverConflict);
    }
    read_report(transaction)
}

fn import_notification_setting(
    transaction: &Transaction<'_>,
    input: &crate::LegacyFeedDiscoveryCutoverInput,
    now_ms: i64,
    committed_revision: pod0_domain::StateRevision,
) -> Result<(), StorageError> {
    let fingerprint = digest_hex(input.backup_digest.into_bytes());
    record_imported_command(
        transaction,
        input.notification_command_id,
        &fingerprint,
        committed_revision,
        now_ms,
    )?;
    transaction
        .execute(
            "UPDATE pod0_new_episode_notification_settings
             SET enabled=?1,revision=?2,updated_at_ms=?3 WHERE singleton=1",
            params![
                i64::from(input.notifications_enabled),
                to_i64(committed_revision.value)?,
                now_ms
            ],
        )
        .map_err(|error| StorageError::sqlite("import notification setting", error))?;
    Ok(())
}

fn import_occurrences(
    transaction: &Transaction<'_>,
    candidates: &[LegacyFeedDiscoveryCandidate],
    now_ms: i64,
    committed_revision: pod0_domain::StateRevision,
) -> Result<(), StorageError> {
    let mut groups: BTreeMap<[u8; 16], Vec<&LegacyFeedDiscoveryCandidate>> = BTreeMap::new();
    for candidate in candidates {
        groups
            .entry(candidate.occurrence_id.into_bytes())
            .or_default()
            .push(candidate);
    }
    for group in groups.into_values() {
        import_occurrence(transaction, &group, now_ms, committed_revision)?;
    }
    Ok(())
}

fn import_occurrence(
    transaction: &Transaction<'_>,
    candidates: &[&LegacyFeedDiscoveryCandidate],
    now_ms: i64,
    committed_revision: pod0_domain::StateRevision,
) -> Result<(), StorageError> {
    let first = candidates
        .first()
        .copied()
        .ok_or(StorageError::InvalidFeedDiscoveryCutover)?;
    let command_fingerprint = occurrence_fingerprint(first);
    record_imported_command(
        transaction,
        first.command_id,
        &command_fingerprint,
        committed_revision,
        now_ms,
    )?;
    let mut episodes: BTreeMap<[u8; 16], &LegacyFeedDiscoveryCandidate> = BTreeMap::new();
    for candidate in candidates {
        episodes
            .entry(candidate.episode_id.into_bytes())
            .or_insert(candidate);
    }
    transaction
        .execute(
            "INSERT INTO pod0_feed_discovery_occurrences(
                occurrence_id,command_id,podcast_id,state,workflow_schema_version,policy_version,
                is_initial_population,item_count,observed_at_ms,created_at_ms,updated_at_ms
             ) VALUES(?1,?2,?3,'pending',1,1,0,?4,?5,?6,?6)",
            params![
                first.occurrence_id.into_bytes().as_slice(),
                first.command_id.into_bytes().as_slice(),
                first.podcast_id.into_bytes().as_slice(),
                to_i64(episodes.len() as u64)?,
                first.observed_at.value(),
                now_ms,
            ],
        )
        .map_err(|error| StorageError::sqlite("import feed-discovery occurrence", error))?;
    for candidate in episodes.values() {
        insert_item(transaction, candidate)?;
    }
    let active = candidates
        .iter()
        .any(|candidate| candidate.disposition == LegacyFeedDiscoveryDisposition::Pending);
    transaction
        .execute(
            "INSERT INTO pod0_feed_discovery_workflows(
                occurrence_id,stage,workflow_revision,expires_at_ms,planned_at_ms,
                completed_at_ms,updated_at_ms
             ) VALUES(?1,?2,1,?3,?4,?5,?4)",
            params![
                first.occurrence_id.into_bytes().as_slice(),
                if active { "active" } else { "succeeded" },
                first.expires_at.value(),
                now_ms,
                (!active).then_some(now_ms),
            ],
        )
        .map_err(|error| StorageError::sqlite("import feed-discovery workflow", error))?;
    for candidate in candidates {
        insert_effect(transaction, candidate, now_ms)?;
    }
    Ok(())
}

fn record_imported_command(
    transaction: &Transaction<'_>,
    command_id: pod0_domain::CommandId,
    fingerprint: &str,
    committed_revision: pod0_domain::StateRevision,
    now_ms: i64,
) -> Result<(), StorageError> {
    if let Some(existing) = command_was_applied(transaction, command_id, fingerprint)? {
        return if existing == committed_revision {
            Ok(())
        } else {
            Err(StorageError::FeedDiscoveryCutoverConflict)
        };
    }
    transaction
        .execute(
            "INSERT INTO pod0_library_commands(command_id,command_fingerprint,applied_revision,\
             completed_at_ms) VALUES(?1,?2,?3,?4)",
            params![
                command_id.into_bytes().as_slice(),
                fingerprint,
                to_i64(committed_revision.value)?,
                now_ms
            ],
        )
        .map_err(|error| StorageError::sqlite("record imported feed command", error))?;
    Ok(())
}

fn insert_item(
    transaction: &Transaction<'_>,
    candidate: &LegacyFeedDiscoveryCandidate,
) -> Result<(), StorageError> {
    let item_id =
        pod0_application::feed_discovery_item_id(candidate.occurrence_id, candidate.episode_id);
    transaction
        .execute(
            "INSERT INTO pod0_feed_discovery_items(
                item_id,occurrence_id,episode_id,input_version,published_at_ms
             ) VALUES(?1,?2,?3,?4,?5)",
            params![
                item_id.into_bytes().as_slice(),
                candidate.occurrence_id.into_bytes().as_slice(),
                candidate.episode_id.into_bytes().as_slice(),
                candidate.input_version,
                candidate.published_at.value(),
            ],
        )
        .map_err(|error| StorageError::sqlite("import feed-discovery item", error))?;
    Ok(())
}

fn insert_effect(
    transaction: &Transaction<'_>,
    candidate: &LegacyFeedDiscoveryCandidate,
    now_ms: i64,
) -> Result<(), StorageError> {
    let (stage, failure) = effect_stage(candidate.disposition);
    let (command_id, cancellation_id) = effect_identity(
        candidate.occurrence_id,
        candidate.episode_id,
        candidate.kind,
    );
    transaction
        .execute(
            "INSERT INTO pod0_feed_discovery_effects(
                occurrence_id,episode_id,kind,stage,command_id,cancellation_id,request_id,
                attempt,not_before_ms,deadline_at_ms,failure_code,created_at_ms,updated_at_ms
             ) VALUES(?1,?2,?3,?4,?5,?6,NULL,?7,?8,NULL,?9,?10,?10)",
            params![
                candidate.occurrence_id.into_bytes().as_slice(),
                candidate.episode_id.into_bytes().as_slice(),
                candidate.kind.wire(),
                stage,
                command_id.map(|value| value.into_bytes().to_vec()),
                cancellation_id.into_bytes().as_slice(),
                i64::from(candidate.attempt),
                candidate.not_before.map(|time| time.value()),
                failure,
                now_ms,
            ],
        )
        .map_err(|error| StorageError::sqlite("import feed-discovery effect", error))?;
    Ok(())
}

include!("feed_discovery_cutover_commit_helpers.rs");
