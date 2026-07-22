use std::collections::BTreeSet;

use pod0_domain::ContentDigest;
use rusqlite::{Transaction, params};
use sha2::{Digest as _, Sha256};

use super::cutover::*;
use super::cutover_stage::transcript_workflow_source_fingerprint;
use super::support::{i64_value, validate_detail, validate_request};
use crate::StorageError;

pub(super) fn validate_input(
    input: &LegacyTranscriptWorkflowCutoverInput,
) -> Result<(), StorageError> {
    if input.source_generation == 0
        || input.now_ms < 0
        || input.max_attempts == 0
        || input.rows.len() > MAX_LEGACY_TRANSCRIPT_WORKFLOW_ROWS
        || input.candidates.len() > MAX_LEGACY_TRANSCRIPT_WORKFLOW_ROWS
        || transcript_workflow_source_fingerprint(&input.rows) != input.source_fingerprint
    {
        return Err(StorageError::TranscriptWorkflowConflict);
    }
    let mut fingerprints = BTreeSet::new();
    let mut episodes = BTreeSet::new();
    let mut total_bytes = 0_u64;
    for row in &input.rows {
        if row.row_bytes.len() > 1_048_576
            || row.row_fingerprint != digest(&row.row_bytes)
            || !fingerprints.insert(row.row_fingerprint.into_bytes())
        {
            return Err(StorageError::TranscriptWorkflowConflict);
        }
        total_bytes = total_bytes
            .checked_add(row.row_bytes.len() as u64)
            .ok_or(StorageError::TranscriptWorkflowConflict)?;
        episodes.insert(row.episode_id);
    }
    if input.backup_byte_count < total_bytes {
        return Err(StorageError::TranscriptWorkflowConflict);
    }
    let mut candidates = BTreeSet::new();
    for candidate in &input.candidates {
        validate_request(&candidate.request)?;
        if !episodes.contains(&candidate.episode_id) || !candidates.insert(candidate.episode_id) {
            return Err(StorageError::TranscriptWorkflowConflict);
        }
        validate_candidate(candidate)?;
    }
    Ok(())
}

fn validate_candidate(candidate: &LegacyTranscriptWorkflowCandidate) -> Result<(), StorageError> {
    let has_attempt = candidate.prepared_attempt.is_some() && candidate.request_id.is_some();
    if candidate.prepared_attempt.is_some() != candidate.request_id.is_some() {
        return Err(StorageError::TranscriptWorkflowConflict);
    }
    match &candidate.disposition {
        LegacyTranscriptWorkflowDisposition::Restart
        | LegacyTranscriptWorkflowDisposition::RecoverProvider { .. }
        | LegacyTranscriptWorkflowDisposition::Ambiguous
            if !has_attempt =>
        {
            Err(StorageError::TranscriptWorkflowConflict)
        }
        LegacyTranscriptWorkflowDisposition::RecoverProvider {
            external_operation_id,
            provider_status,
        } => {
            if external_operation_id.is_empty()
                || external_operation_id.len() > 1_024
                || provider_status
                    .as_ref()
                    .is_some_and(|value| value.len() > 1_024)
            {
                Err(StorageError::TranscriptWorkflowConflict)
            } else {
                Ok(())
            }
        }
        LegacyTranscriptWorkflowDisposition::Blocked {
            failure_code,
            failure_detail,
            ..
        }
        | LegacyTranscriptWorkflowDisposition::Failed {
            failure_code,
            failure_detail,
            ..
        } => {
            if failure_code.is_empty() || failure_code.len() > 256 {
                return Err(StorageError::TranscriptWorkflowConflict);
            }
            validate_detail(failure_detail.as_deref())
        }
        LegacyTranscriptWorkflowDisposition::IndexPending {
            evidence_input_version,
            ..
        }
        | LegacyTranscriptWorkflowDisposition::IndexSucceeded {
            evidence_input_version,
            ..
        } if evidence_input_version.is_empty() || evidence_input_version.len() > 256 => {
            Err(StorageError::TranscriptWorkflowConflict)
        }
        _ => Ok(()),
    }
}

pub(super) fn insert_manifest(
    transaction: &Transaction<'_>,
    input: &LegacyTranscriptWorkflowCutoverInput,
) -> Result<(), StorageError> {
    transaction
        .execute(
            "INSERT INTO pod0_transcript_workflow_imports(singleton,source_generation,
             source_fingerprint,backup_digest,backup_byte_count,row_count,state,staged_at_ms)
             VALUES(1,?1,?2,?3,?4,?5,'staged',?6)",
            params![
                i64_value(input.source_generation)?,
                input.source_fingerprint.into_bytes().as_slice(),
                input.backup_digest.into_bytes().as_slice(),
                i64_value(input.backup_byte_count)?,
                i64::try_from(input.rows.len())
                    .map_err(|_| StorageError::TranscriptWorkflowConflict)?,
                input.now_ms
            ],
        )
        .map_err(|error| StorageError::sqlite("insert transcript workflow import", error))?;
    Ok(())
}

pub(super) fn insert_backup_rows(
    transaction: &Transaction<'_>,
    input: &LegacyTranscriptWorkflowCutoverInput,
) -> Result<(), StorageError> {
    let mut statement = transaction
        .prepare(
            "INSERT INTO pod0_transcript_workflow_import_rows(source_generation,ordinal,
             row_fingerprint,row_bytes,classification,episode_id) VALUES(?1,?2,?3,?4,?5,?6)",
        )
        .map_err(|error| StorageError::sqlite("prepare transcript workflow backup rows", error))?;
    for (ordinal, row) in input.rows.iter().enumerate() {
        statement
            .execute(params![
                i64_value(input.source_generation)?,
                i64::try_from(ordinal).map_err(|_| StorageError::TranscriptWorkflowConflict)?,
                row.row_fingerprint.into_bytes().as_slice(),
                row.row_bytes,
                row.classification.wire(),
                row.episode_id.into_bytes().as_slice()
            ])
            .map_err(|error| {
                StorageError::sqlite("insert transcript workflow backup row", error)
            })?;
    }
    Ok(())
}

fn digest(value: &[u8]) -> ContentDigest {
    let mut hash = Sha256::new();
    hash.update(value);
    ContentDigest::from_bytes(hash.finalize().into())
}
