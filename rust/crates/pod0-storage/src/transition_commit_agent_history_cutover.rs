use pod0_application::{
    LegacyBusinessCutoverActivityInput, LegacyBusinessCutoverDomain, RequestDisposition,
    plan_legacy_business_cutover,
};
use pod0_domain::{CommandId, ContentDigest, StateRevision, UnixTimestampMilliseconds};
use rusqlite::params;
use sha2::{Digest as _, Sha256};

use super::TransitionCommit;
use crate::agent_history_cutover_read::{matching_report, read_evidence};
use crate::agent_history_cutover_validation::{validate_input, verify_staged};
use crate::{
    AgentHistoryCutoverState, LegacyAgentHistoryCutoverInput, LegacyAgentHistoryCutoverReport,
    StorageError, agent_history_counts, agent_history_source_fingerprint,
    agent_history_source_generation,
};

pub(crate) fn commit_agent_history_cutover_stage(
    path: &std::path::Path,
    input: LegacyAgentHistoryCutoverInput,
) -> Result<LegacyAgentHistoryCutoverReport, StorageError> {
    validate_input(&input)?;
    let fingerprint = agent_history_source_fingerprint(&input);
    let generation = agent_history_source_generation(fingerprint);
    commit(
        path,
        "stage",
        generation,
        fingerprint,
        input.observed_at,
        false,
        |tx| {
            if let Some(report) = read_evidence(tx)? {
                let counts = agent_history_counts(&input.conversations);
                if report.state.source_generation() == Some(generation)
                    && report.source_fingerprint == Some(fingerprint)
                    && report.backup_digest == Some(input.backup_digest)
                    && report.backup_byte_count == Some(input.backup_byte_count)
                    && report.conversation_count == counts.0 as u32
                    && report.turn_count == counts.1 as u32
                    && report.message_count == counts.2 as u32
                {
                    return Ok(false);
                }
                return Err(StorageError::AgentTurnConflict);
            }
            crate::agent_history_cutover::ensure_empty_staging(tx)?;
            crate::agent_history_cutover::stage_rows(tx, &input)?;
            let (conversations, turns, messages) = agent_history_counts(&input.conversations);
            tx.execute(
                "INSERT INTO pod0_agent_history_cutover_evidence(singleton,state,source_generation,\
             source_fingerprint,backup_digest,backup_byte_count,conversation_count,turn_count,\
             message_count,staged_at_ms,verified_at_ms,committed_at_ms) \
             VALUES(1,'staged',?1,?2,?3,?4,?5,?6,?7,?8,NULL,NULL)",
                params![
                    crate::agent_history_cutover::to_i64(generation)?,
                    fingerprint.into_bytes().as_slice(),
                    input.backup_digest.into_bytes().as_slice(),
                    crate::agent_history_cutover::to_i64(input.backup_byte_count)?,
                    crate::agent_history_cutover::to_i64(conversations as u64)?,
                    crate::agent_history_cutover::to_i64(turns as u64)?,
                    crate::agent_history_cutover::to_i64(messages as u64)?,
                    input.observed_at.value()
                ],
            )
            .map_err(|error| StorageError::sqlite("stage agent history cutover evidence", error))?;
            Ok(true)
        },
    )?;
    read_report(path)
}

pub(crate) fn commit_agent_history_cutover_verify(
    path: &std::path::Path,
    generation: u64,
    observed_at: UnixTimestampMilliseconds,
) -> Result<LegacyAgentHistoryCutoverReport, StorageError> {
    commit(
        path,
        "verify",
        generation,
        phase_fingerprint("verify", generation),
        observed_at,
        false,
        |tx| {
            let report = matching_report(tx, generation)?;
            if matches!(
                report.state,
                AgentHistoryCutoverState::Authoritative { .. }
                    | AgentHistoryCutoverState::Verified { .. }
            ) {
                return Ok(false);
            }
            verify_staged(tx, &report)?;
            tx.execute("UPDATE pod0_agent_history_cutover_evidence SET state='verified',verified_at_ms=?1 WHERE singleton=1 AND state='staged'", [observed_at.value()]).map_err(|error| StorageError::sqlite("verify agent history cutover", error))?;
            Ok(true)
        },
    )?;
    read_report(path)
}

pub(crate) fn commit_agent_history_cutover_authority(
    path: &std::path::Path,
    generation: u64,
    observed_at: UnixTimestampMilliseconds,
) -> Result<LegacyAgentHistoryCutoverReport, StorageError> {
    commit(
        path,
        "authority",
        generation,
        phase_fingerprint("authority", generation),
        observed_at,
        true,
        |tx| {
            let report = matching_report(tx, generation)?;
            if matches!(report.state, AgentHistoryCutoverState::Authoritative { .. }) {
                return Ok(false);
            }
            if !matches!(report.state, AgentHistoryCutoverState::Verified { .. }) {
                return Err(StorageError::AgentTurnConflict);
            }
            verify_staged(tx, &report)?;
            crate::agent_history_cutover::commit_rows(tx, observed_at.value())?;
            crate::agent_history_cutover::clear_staged(tx)?;
            tx.execute("UPDATE pod0_agent_history_cutover_evidence SET state='authoritative',committed_at_ms=?1 WHERE singleton=1 AND state='verified'", [observed_at.value()]).map_err(|error| StorageError::sqlite("commit agent history cutover", error))?;
            if tx.changes() != 1 {
                return Err(StorageError::AgentTurnConflict);
            }
            Ok(true)
        },
    )?;
    read_report(path)
}

pub(crate) fn commit_agent_history_cutover_discard(
    path: &std::path::Path,
    generation: u64,
) -> Result<bool, StorageError> {
    let observed_at = UnixTimestampMilliseconds::new(0);
    let receipt = commit(
        path,
        "discard",
        generation,
        phase_fingerprint("discard", generation),
        observed_at,
        false,
        |tx| {
            let Some(report) = read_evidence(tx)? else {
                return Ok(false);
            };
            if report.state.source_generation() != Some(generation)
                || matches!(report.state, AgentHistoryCutoverState::Authoritative { .. })
            {
                return Err(StorageError::AgentTurnConflict);
            }
            crate::agent_history_cutover::clear_staged(tx)?;
            tx.execute(
                "DELETE FROM pod0_agent_history_cutover_evidence WHERE singleton=1",
                [],
            )
            .map_err(|error| StorageError::sqlite("discard agent history cutover", error))?;
            Ok(true)
        },
    )?;
    Ok(receipt.disposition == RequestDisposition::Accepted)
}

fn commit<F>(
    path: &std::path::Path,
    phase: &str,
    generation: u64,
    fingerprint: ContentDigest,
    at: UnixTimestampMilliseconds,
    authority: bool,
    mutate: F,
) -> Result<crate::CommitReceipt, StorageError>
where
    F: FnOnce(&rusqlite::Transaction<'_>) -> Result<bool, StorageError>,
{
    let command_id = phase_command("agent-history", phase, generation);
    TransitionCommit::open(path)?.commit_planned_with(
        crate::TransitionIngress {
            kind: crate::TransitionIngressKind::Migration,
            id: command_id.into_bytes(),
            fingerprint,
        },
        at,
        |tx| {
            let current = core_revision(tx)?;
            let changes = preview_agent_phase(tx, phase, generation)?;
            let committed = if changes {
                StateRevision::new(
                    current
                        .value
                        .checked_add(1)
                        .ok_or(StorageError::InvalidActivity)?,
                )
            } else {
                current
            };
            plan_legacy_business_cutover(LegacyBusinessCutoverActivityInput {
                command_id,
                current_revision: current,
                committed_revision: committed,
                domain: LegacyBusinessCutoverDomain::AgentHistory,
                disposition: if changes {
                    RequestDisposition::Accepted
                } else {
                    RequestDisposition::NoSemanticChange
                },
                authority_cutover: authority && changes,
            })
            .map(|plan| plan.map_mutation(|_| (changes, committed)))
            .map_err(|_| StorageError::InvalidActivity)
        },
        |tx, expected, (changes, committed)| {
            let actual_changes = mutate(tx)?;
            if actual_changes != changes {
                return Err(StorageError::RevisionConflict);
            }
            if !changes {
                return Ok(expected);
            }
            let actual = crate::library_store::advance_playback_revision(tx)?;
            if actual != committed {
                return Err(StorageError::RevisionConflict);
            }
            Ok(committed)
        },
    )
}

fn preview_agent_phase(
    tx: &rusqlite::Connection,
    phase: &str,
    generation: u64,
) -> Result<bool, StorageError> {
    let report = read_evidence(tx)?;
    match phase {
        "stage" => Ok(report.is_none()),
        "verify" => Ok(report.is_some_and(|r| {
            matches!(r.state, AgentHistoryCutoverState::Staged { .. })
                && r.state.source_generation() == Some(generation)
        })),
        "authority" => Ok(report.is_some_and(|r| {
            matches!(r.state, AgentHistoryCutoverState::Verified { .. })
                && r.state.source_generation() == Some(generation)
        })),
        "discard" => Ok(report.is_some()),
        _ => Err(StorageError::InvalidActivity),
    }
}
fn read_report(path: &std::path::Path) -> Result<LegacyAgentHistoryCutoverReport, StorageError> {
    crate::LibraryStore::open_authoritative(path)?.agent_history_cutover_report()
}
fn core_revision(c: &rusqlite::Connection) -> Result<StateRevision, StorageError> {
    let v: i64 = c
        .query_row(
            "SELECT state_revision FROM pod0_playback_state WHERE singleton=1",
            [],
            |r| r.get(0),
        )
        .map_err(|e| StorageError::sqlite("read agent cutover revision", e))?;
    Ok(StateRevision::new(
        u64::try_from(v).map_err(|_| StorageError::InvalidActivity)?,
    ))
}
fn phase_command(domain: &str, phase: &str, generation: u64) -> CommandId {
    let mut h = Sha256::new();
    h.update(b"pod0/legacy-business-cutover/v1");
    h.update(domain.as_bytes());
    h.update(phase.as_bytes());
    h.update(generation.to_be_bytes());
    CommandId::from_bytes(h.finalize()[..16].try_into().expect("digest prefix"))
}
fn phase_fingerprint(phase: &str, generation: u64) -> ContentDigest {
    let mut h = Sha256::new();
    h.update(b"pod0/legacy-business-cutover/fingerprint/v1");
    h.update(phase.as_bytes());
    h.update(generation.to_be_bytes());
    ContentDigest::from_bytes(h.finalize().into())
}
