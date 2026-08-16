use pod0_domain::{CancellationId, UnixTimestampMilliseconds};
use rusqlite::params;

use crate::StorageError;

pub(super) fn cancel_chapter_workflows(
    transaction: &rusqlite::Transaction<'_>,
    cancellation_id: CancellationId,
    committed_at: UnixTimestampMilliseconds,
) -> Result<(), StorageError> {
    transaction
        .execute(
            "UPDATE pod0_publisher_chapter_workflows SET state='cancelled',
             workflow_revision=workflow_revision+1,deadline_at_ms=NULL,not_before_ms=NULL,
             failure_code='cancelled',failure_detail=NULL,updated_at_ms=?1
             WHERE cancellation_id=?2 AND state IN('requested','retry_scheduled')",
            params![committed_at.value, cancellation_id.into_bytes().as_slice()],
        )
        .map_err(|error| StorageError::sqlite("cancel publisher chapter workflow", error))?;
    transaction
        .execute(
            "UPDATE pod0_model_chapter_workflows SET state='cancelled',
             workflow_revision=workflow_revision+1,deadline_at_ms=NULL,not_before_ms=NULL,
             failure_code='cancelled',failure_detail=NULL,updated_at_ms=?1
             WHERE cancellation_id=?2 AND state IN('awaiting_transcript','awaiting_publisher',
             'requested','submission_authorized','provider_accepted','ambiguous',
             'retry_scheduled','blocked','failed')",
            params![committed_at.value, cancellation_id.into_bytes().as_slice()],
        )
        .map_err(|error| StorageError::sqlite("cancel model chapter workflow", error))?;
    Ok(())
}
