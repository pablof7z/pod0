use rusqlite::{Connection, OptionalExtension};

use super::cutover::TranscriptWorkflowAuthorityState;
use crate::StorageError;

pub(super) fn read_authority(
    connection: &Connection,
) -> Result<TranscriptWorkflowAuthorityState, StorageError> {
    let import: Option<(i64, String)> = connection
        .query_row(
            "SELECT source_generation,state FROM pod0_transcript_workflow_imports WHERE singleton=1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read transcript workflow import authority", error))?;
    let cutover: Option<(i64, String)> = connection
        .query_row(
            "SELECT source_generation,state FROM pod0_domain_cutovers
             WHERE domain='transcript_workflows'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read transcript workflow cutover", error))?;
    match (import, cutover) {
        (None, None) => Ok(TranscriptWorkflowAuthorityState::NotStarted),
        (Some((generation, import_state)), Some((cutover_generation, cutover_state)))
            if generation == cutover_generation =>
        {
            let source_generation =
                u64::try_from(generation).map_err(|_| StorageError::TranscriptWorkflowConflict)?;
            match (import_state.as_str(), cutover_state.as_str()) {
                ("staged", "staged") => {
                    Ok(TranscriptWorkflowAuthorityState::Staged { source_generation })
                }
                ("verified", "staged") => {
                    Ok(TranscriptWorkflowAuthorityState::Verified { source_generation })
                }
                ("authoritative", "authoritative") => {
                    Ok(TranscriptWorkflowAuthorityState::Authoritative { source_generation })
                }
                _ => Err(StorageError::TranscriptWorkflowConflict),
            }
        }
        _ => Err(StorageError::TranscriptWorkflowConflict),
    }
}

pub(super) fn require_authoritative(connection: &Connection) -> Result<(), StorageError> {
    if read_authority(connection)?.is_authoritative() {
        Ok(())
    } else {
        Err(StorageError::CutoverNotAuthoritative)
    }
}
