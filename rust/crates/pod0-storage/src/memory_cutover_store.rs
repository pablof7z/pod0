use pod0_domain::ContentDigest;
use rusqlite::{Connection, OptionalExtension, Transaction, params};

// Stage/verify/discard operate only on inactive migration rows. The read model projects them as
// empty; the sole authority flip is the typed `TransitionCommit` reached by `commit_*` below.

use crate::{
    LegacyMemoryCutoverInput, LegacyMemoryCutoverReport, LibraryStore, MemoryCutoverState,
    StorageError, memory_source_fingerprint, memory_source_generation,
    validate_memory_cutover_input,
};

impl LibraryStore {
    pub fn memory_cutover_report(&self) -> Result<LegacyMemoryCutoverReport, StorageError> {
        self.read(read_report)
    }

    pub fn stage_legacy_memory_cutover(
        &self,
        input: LegacyMemoryCutoverInput,
    ) -> Result<LegacyMemoryCutoverReport, StorageError> {
        validate_memory_cutover_input(&input)?;
        let fingerprint = memory_source_fingerprint(&input)?;
        let generation = memory_source_generation(fingerprint);
        self.write(|transaction| {
            if let Some(report) = read_evidence(transaction)? {
                if report.state.source_generation() == Some(generation)
                    && report.source_fingerprint == Some(fingerprint)
                    && report.backup_digest == Some(input.backup_digest)
                    && report.backup_byte_count == Some(input.backup_byte_count)
                {
                    return Ok(report);
                }
                return Err(StorageError::RevisionConflict);
            }
            ensure_inactive_empty(transaction)?;
            stage_rows(transaction, &input)?;
            let deleted_count = input
                .memories
                .iter()
                .filter(|memory| memory.deleted)
                .count();
            transaction
                .execute(
                    "INSERT INTO pod0_memory_cutover_evidence(singleton,state,source_generation,\
                     source_fingerprint,backup_digest,backup_byte_count,memory_count,deleted_count,\
                     compiled_present,staged_at_ms,verified_at_ms,committed_at_ms) \
                     VALUES(1,'staged',?1,?2,?3,?4,?5,?6,?7,?8,NULL,NULL)",
                    params![
                        to_i64(generation)?,
                        fingerprint.into_bytes().as_slice(),
                        input.backup_digest.into_bytes().as_slice(),
                        to_i64(input.backup_byte_count)?,
                        to_i64(input.memories.len() as u64)?,
                        to_i64(deleted_count as u64)?,
                        i64::from(input.compiled.is_some()),
                        input.observed_at.value(),
                    ],
                )
                .map_err(|error| StorageError::sqlite("stage memory cutover evidence", error))?;
            transaction
                .execute(
                    "UPDATE pod0_memory_state SET source_generation=?1 WHERE singleton=1",
                    [to_i64(generation)?],
                )
                .map_err(|error| StorageError::sqlite("stage memory authority", error))?;
            read_evidence(transaction)?.ok_or(StorageError::RevisionConflict)
        })
    }

    pub fn verify_legacy_memory_cutover(
        &self,
        source_generation: u64,
        observed_at_ms: i64,
    ) -> Result<LegacyMemoryCutoverReport, StorageError> {
        self.write(|transaction| {
            let report = matching_report(transaction, source_generation)?;
            if matches!(report.state, MemoryCutoverState::Authoritative { .. }) {
                return Ok(report);
            }
            verify_rows(transaction, &report)?;
            if matches!(report.state, MemoryCutoverState::Staged { .. }) {
                transaction
                    .execute(
                        "UPDATE pod0_memory_cutover_evidence SET state='verified',\
                         verified_at_ms=?1 WHERE singleton=1 AND state='staged'",
                        [observed_at_ms],
                    )
                    .map_err(|error| StorageError::sqlite("verify memory cutover", error))?;
            }
            read_evidence(transaction)?.ok_or(StorageError::RevisionConflict)
        })
    }

    pub fn commit_legacy_memory_cutover(
        &self,
        source_generation: u64,
        observed_at_ms: i64,
    ) -> Result<LegacyMemoryCutoverReport, StorageError> {
        crate::transition_commit::commit_memory_cutover(
            self.path(),
            source_generation,
            observed_at_ms,
        )
    }

    pub fn discard_staged_legacy_memory_cutover(
        &self,
        source_generation: u64,
    ) -> Result<bool, StorageError> {
        self.write(|transaction| {
            let Some(report) = read_evidence(transaction)? else {
                return Ok(false);
            };
            if report.state.source_generation() != Some(source_generation)
                || matches!(report.state, MemoryCutoverState::Authoritative { .. })
            {
                return Err(StorageError::RevisionConflict);
            }
            clear_rows(transaction)?;
            transaction
                .execute(
                    "DELETE FROM pod0_memory_cutover_evidence WHERE singleton=1",
                    [],
                )
                .map_err(|error| StorageError::sqlite("discard memory evidence", error))?;
            transaction
                .execute(
                    "UPDATE pod0_memory_state SET source_generation=NULL WHERE singleton=1",
                    [],
                )
                .map_err(|error| StorageError::sqlite("discard memory generation", error))?;
            Ok(true)
        })
    }
}

include!("memory_cutover_store_rows.rs");
