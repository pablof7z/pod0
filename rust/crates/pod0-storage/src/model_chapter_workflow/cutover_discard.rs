use pod0_domain::{CancellationId, CommandId};
use rusqlite::{OptionalExtension, params};

use super::cutover::{CUTOVER_DOMAIN, ModelChapterWorkflowAuthorityState, read_authority};
use super::support::i64_value;
use crate::{LibraryStore, StorageError};

const STAGED_RECORD_PREDICATE: &str = "command_id=?1 AND cancellation_id=?2 \
    AND issued_revision=?3 AND created_at_ms=?4 AND updated_at_ms=?4 \
    AND generation=1 AND workflow_revision=1 AND attempt=1 AND replan_pending=0 \
    AND deadline_at_ms IS NULL AND not_before_ms IS NULL \
    AND provider_operation_id IS NULL AND provider_status IS NULL \
    AND state IN('succeeded','ambiguous','blocked','failed','cancelled')";

impl LibraryStore {
    pub fn discard_staged_legacy_model_chapter_workflow_cutover(
        &self,
        source_generation: u64,
        command_id: CommandId,
        cancellation_id: CancellationId,
    ) -> Result<ModelChapterWorkflowAuthorityState, StorageError> {
        if source_generation == 0 {
            return Err(StorageError::ChapterWorkflowConflict);
        }
        self.write(|transaction| {
            match read_authority(transaction)? {
                ModelChapterWorkflowAuthorityState::Staged {
                    source_generation: staged,
                } if staged == source_generation => {}
                _ => return Err(StorageError::ChapterWorkflowConflict),
            }
            let generation = i64_value(source_generation)?;
            let fence: Option<(i64, i64)> = transaction
                .query_row(
                    "SELECT core_revision,committed_at_ms FROM pod0_domain_cutovers \
                     WHERE domain=?1 AND state='staged' AND source_generation=?2",
                    params![CUTOVER_DOMAIN, generation],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()
                .map_err(|error| {
                    StorageError::sqlite("read staged model workflow cutover fence", error)
                })?;
            let Some((core_revision, staged_at_ms)) = fence else {
                return Err(StorageError::ChapterWorkflowConflict);
            };
            let command = command_id.into_bytes();
            let cancellation = cancellation_id.into_bytes();
            let inspection_sql = format!(
                "SELECT COUNT(*),COALESCE(SUM(CASE WHEN {STAGED_RECORD_PREDICATE} \
                 THEN 1 ELSE 0 END),0) FROM pod0_model_chapter_workflows"
            );
            let (total, staged): (i64, i64) = transaction
                .query_row(
                    &inspection_sql,
                    params![
                        command.as_slice(),
                        cancellation.as_slice(),
                        core_revision,
                        staged_at_ms,
                    ],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .map_err(|error| {
                    StorageError::sqlite("verify staged model workflow records", error)
                })?;
            if total != staged {
                return Err(StorageError::ChapterWorkflowConflict);
            }
            let delete_sql =
                format!("DELETE FROM pod0_model_chapter_workflows WHERE {STAGED_RECORD_PREDICATE}");
            let deleted = transaction
                .execute(
                    &delete_sql,
                    params![
                        command.as_slice(),
                        cancellation.as_slice(),
                        core_revision,
                        staged_at_ms,
                    ],
                )
                .map_err(|error| {
                    StorageError::sqlite("discard staged model workflow records", error)
                })?;
            if i64::try_from(deleted).ok() != Some(total) {
                return Err(StorageError::ChapterWorkflowConflict);
            }
            let deleted_marker = transaction
                .execute(
                    "DELETE FROM pod0_domain_cutovers WHERE domain=?1 AND state='staged' \
                     AND source_generation=?2",
                    params![CUTOVER_DOMAIN, generation],
                )
                .map_err(|error| {
                    StorageError::sqlite("discard staged model workflow cutover marker", error)
                })?;
            if deleted_marker != 1 {
                return Err(StorageError::ChapterWorkflowConflict);
            }
            Ok(ModelChapterWorkflowAuthorityState::NotStarted)
        })
    }
}
