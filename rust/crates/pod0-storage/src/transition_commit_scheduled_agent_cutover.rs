use pod0_application::{
    LegacyBusinessCutoverActivityInput, LegacyBusinessCutoverDomain, RequestDisposition,
    plan_legacy_business_cutover,
};
use pod0_domain::{CommandId, ContentDigest, StateRevision, UnixTimestampMilliseconds};
use rusqlite::params;
use sha2::{Digest as _, Sha256};

use super::TransitionCommit;
use crate::scheduled_agent_cutover_read::{matching_report, read_evidence};
use crate::scheduled_agent_cutover_validation::{validate_input, verify_staged_rows};
use crate::{
    LegacyScheduledAgentCutoverInput, LegacyScheduledAgentCutoverReport,
    ScheduledAgentAuthorityState, ScheduledAgentCutoverState, StorageError,
    scheduled_agent_cutover_source_fingerprint, scheduled_agent_cutover_source_generation,
};

pub(crate) fn commit_scheduled_agent_cutover_stage(
    path: &std::path::Path,
    input: LegacyScheduledAgentCutoverInput,
) -> Result<LegacyScheduledAgentCutoverReport, StorageError> {
    validate_input(&input)?;
    let fingerprint = scheduled_agent_cutover_source_fingerprint(&input);
    let generation = scheduled_agent_cutover_source_generation(fingerprint);
    commit(
        path,
        "stage",
        generation,
        fingerprint,
        input.observed_at,
        false,
        |tx| {
            crate::scheduled_agent_cutover::require_inactive(tx)?;
            if let Some(report) = read_evidence(tx)? {
                if report.state.source_generation() == Some(generation)
                    && report.source_fingerprint == Some(fingerprint)
                    && report.backup_digest == Some(input.backup_digest)
                    && report.backup_byte_count == Some(input.backup_byte_count)
                    && report.task_count == input.tasks.len() as u32
                    && report.occurrence_count == input.occurrences.len() as u32
                {
                    return Ok(false);
                }
                return Err(StorageError::ScheduledAgentWorkflowConflict);
            }
            crate::scheduled_agent_cutover::ensure_empty_target(tx)?;
            crate::scheduled_agent_cutover_stage::stage_rows(tx, &input)?;
            tx.execute("INSERT INTO pod0_scheduled_agent_cutover_evidence(singleton,state,source_generation,source_fingerprint,backup_digest,backup_byte_count,task_count,occurrence_count,staged_at_ms,verified_at_ms,committed_at_ms) VALUES(1,'staged',?1,?2,?3,?4,?5,?6,?7,NULL,NULL)", params![crate::scheduled_agent_cutover::to_i64(generation)?,fingerprint.into_bytes().as_slice(),input.backup_digest.into_bytes().as_slice(),crate::scheduled_agent_cutover::to_i64(input.backup_byte_count)?,crate::scheduled_agent_cutover::to_i64(input.tasks.len() as u64)?,crate::scheduled_agent_cutover::to_i64(input.occurrences.len() as u64)?,input.observed_at.value()]).map_err(|e|StorageError::sqlite("stage scheduled-agent cutover evidence",e))?;
            Ok(true)
        },
    )?;
    read_report(path)
}

pub(crate) fn commit_scheduled_agent_cutover_verify(
    path: &std::path::Path,
    generation: u64,
    at: UnixTimestampMilliseconds,
) -> Result<LegacyScheduledAgentCutoverReport, StorageError> {
    commit(
        path,
        "verify",
        generation,
        phase_fingerprint("verify", generation),
        at,
        false,
        |tx| {
            crate::scheduled_agent_cutover::require_inactive(tx)?;
            let report = matching_report(tx, generation)?;
            if matches!(report.state, ScheduledAgentCutoverState::Verified { .. }) {
                return Ok(false);
            }
            verify_staged_rows(tx, &report)?;
            tx.execute("UPDATE pod0_scheduled_agent_cutover_evidence SET state='verified',verified_at_ms=?1 WHERE singleton=1 AND state='staged'",[at.value()]).map_err(|e|StorageError::sqlite("verify scheduled-agent cutover",e))?;
            Ok(true)
        },
    )?;
    read_report(path)
}

pub(crate) fn commit_scheduled_agent_cutover_authority(
    path: &std::path::Path,
    generation: u64,
    at: UnixTimestampMilliseconds,
) -> Result<LegacyScheduledAgentCutoverReport, StorageError> {
    commit(
        path,
        "authority",
        generation,
        phase_fingerprint("authority", generation),
        at,
        true,
        |tx| {
            match crate::scheduled_agent_store::read_authority(tx)? {
                ScheduledAgentAuthorityState::Authoritative { source_generation }
                    if source_generation == generation =>
                {
                    return Ok(false);
                }
                ScheduledAgentAuthorityState::Authoritative { .. } => {
                    return Err(StorageError::ScheduledAgentWorkflowConflict);
                }
                ScheduledAgentAuthorityState::Inactive => {}
            }
            let report = matching_report(tx, generation)?;
            if !matches!(report.state, ScheduledAgentCutoverState::Verified { .. }) {
                return Err(StorageError::ScheduledAgentWorkflowConflict);
            }
            verify_staged_rows(tx, &report)?;
            tx.execute("UPDATE pod0_scheduled_agent_authority SET state='authoritative',source_generation=?1,committed_at_ms=?2 WHERE singleton=1 AND state='inactive'",params![crate::scheduled_agent_cutover::to_i64(generation)?,at.value()]).map_err(|e|StorageError::sqlite("commit scheduled-agent authority",e))?;
            if tx.changes() != 1 {
                return Err(StorageError::ScheduledAgentWorkflowConflict);
            }
            tx.execute("UPDATE pod0_scheduled_agent_cutover_evidence SET state='authoritative',committed_at_ms=?1 WHERE singleton=1 AND state='verified'",[at.value()]).map_err(|e|StorageError::sqlite("commit scheduled-agent cutover evidence",e))?;
            if tx.changes() != 1 {
                return Err(StorageError::ScheduledAgentWorkflowConflict);
            }
            Ok(true)
        },
    )?;
    read_report(path)
}

pub(crate) fn commit_scheduled_agent_cutover_discard(
    path: &std::path::Path,
    generation: u64,
) -> Result<bool, StorageError> {
    let at = UnixTimestampMilliseconds::new(0);
    let receipt = commit(
        path,
        "discard",
        generation,
        phase_fingerprint("discard", generation),
        at,
        false,
        |tx| {
            crate::scheduled_agent_cutover::require_inactive(tx)?;
            let Some(report) = read_evidence(tx)? else {
                return Ok(false);
            };
            if report.state.source_generation() != Some(generation) {
                return Err(StorageError::ScheduledAgentWorkflowConflict);
            }
            crate::scheduled_agent_cutover_stage::clear_staged_rows(tx)?;
            tx.execute(
                "DELETE FROM pod0_scheduled_agent_cutover_evidence WHERE singleton=1",
                [],
            )
            .map_err(|e| StorageError::sqlite("discard scheduled-agent cutover", e))?;
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
    let command_id = phase_command(phase, generation);
    TransitionCommit::open(path)?.commit_planned_with(
        crate::TransitionIngress {
            kind: crate::TransitionIngressKind::Migration,
            id: command_id.into_bytes(),
            fingerprint,
        },
        at,
        |tx| {
            let current = core_revision(tx)?;
            let changes = preview(tx, phase, generation)?;
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
                domain: LegacyBusinessCutoverDomain::ScheduledAgent,
                disposition: if changes {
                    RequestDisposition::Accepted
                } else {
                    RequestDisposition::NoSemanticChange
                },
                authority_cutover: authority && changes,
            })
            .map(|p| p.map_mutation(|_| (changes, committed)))
            .map_err(|_| StorageError::InvalidActivity)
        },
        |tx, expected, (changes, committed)| {
            if mutate(tx)? != changes {
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
fn preview(tx: &rusqlite::Connection, phase: &str, generation: u64) -> Result<bool, StorageError> {
    let report = read_evidence(tx)?;
    match phase {
        "stage" => Ok(report.is_none()),
        "verify" => Ok(report.is_some_and(|r| {
            matches!(r.state, ScheduledAgentCutoverState::Staged { .. })
                && r.state.source_generation() == Some(generation)
        })),
        "authority" => Ok(report.is_some_and(|r| {
            matches!(r.state, ScheduledAgentCutoverState::Verified { .. })
                && r.state.source_generation() == Some(generation)
        })),
        "discard" => Ok(report.is_some()),
        _ => Err(StorageError::InvalidActivity),
    }
}
fn read_report(path: &std::path::Path) -> Result<LegacyScheduledAgentCutoverReport, StorageError> {
    crate::LibraryStore::open_authoritative(path)?.scheduled_agent_cutover_report()
}
fn core_revision(c: &rusqlite::Connection) -> Result<StateRevision, StorageError> {
    let v: i64 = c
        .query_row(
            "SELECT state_revision FROM pod0_playback_state WHERE singleton=1",
            [],
            |r| r.get(0),
        )
        .map_err(|e| StorageError::sqlite("read scheduled cutover revision", e))?;
    Ok(StateRevision::new(
        u64::try_from(v).map_err(|_| StorageError::InvalidActivity)?,
    ))
}
fn phase_command(phase: &str, generation: u64) -> CommandId {
    let mut h = Sha256::new();
    h.update(b"pod0/legacy-scheduled-cutover/v1");
    h.update(phase.as_bytes());
    h.update(generation.to_be_bytes());
    CommandId::from_bytes(h.finalize()[..16].try_into().expect("digest prefix"))
}
fn phase_fingerprint(phase: &str, generation: u64) -> ContentDigest {
    let mut h = Sha256::new();
    h.update(b"pod0/legacy-scheduled-cutover/fingerprint/v1");
    h.update(phase.as_bytes());
    h.update(generation.to_be_bytes());
    ContentDigest::from_bytes(h.finalize().into())
}
