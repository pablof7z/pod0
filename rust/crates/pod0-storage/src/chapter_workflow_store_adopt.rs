use pod0_domain::{CancellationId, ChapterArtifactSource, CommandId, EpisodeId, StateRevision};
use rusqlite::{Transaction, params};

use crate::chapter_workflow_store_read::read_workflow;
use crate::chapter_workflow_store_support::i64_value;
use crate::{PublisherChapterWorkflowRecord, StorageError};

#[allow(clippy::too_many_arguments)]
pub(crate) fn adopt_current_publisher_artifact(
    transaction: &Transaction<'_>,
    episode_id: EpisodeId,
    source_url: &str,
    source_version: &str,
    command_id: CommandId,
    cancellation_id: CancellationId,
    issued_revision: StateRevision,
    now_ms: i64,
    max_attempts: u16,
) -> Result<Option<PublisherChapterWorkflowRecord>, StorageError> {
    let Some(selected) = crate::chapter_store_read_selection::read_selected_chapter_artifact(
        transaction,
        episode_id,
    )?
    else {
        return Ok(None);
    };
    if selected.artifact.provenance.source != ChapterArtifactSource::Publisher
        || selected.artifact.source_revision != source_version
    {
        return Ok(None);
    }

    transaction
        .execute(
            "INSERT INTO pod0_publisher_chapter_workflows(episode_id,source_url,\
             source_version,state,generation,workflow_revision,attempt,max_attempts,command_id,\
             cancellation_id,request_id,issued_revision,expected_selection_revision,\
             deadline_at_ms,not_before_ms,selected_artifact_id,failure_code,failure_detail,\
             created_at_ms,updated_at_ms) VALUES(?1,?2,?3,'succeeded',1,1,1,?4,?5,?6,NULL,\
             ?7,?8,NULL,NULL,?9,NULL,NULL,?10,?10)",
            params![
                episode_id.into_bytes().as_slice(),
                source_url,
                source_version,
                i64::from(max_attempts),
                command_id.into_bytes().as_slice(),
                cancellation_id.into_bytes().as_slice(),
                i64_value(issued_revision.value)?,
                i64_value(selected.selection_revision.value)?,
                selected.artifact.artifact_id.into_bytes().as_slice(),
                now_ms,
            ],
        )
        .map_err(|error| StorageError::sqlite("adopt publisher chapter workflow", error))?;
    read_workflow(transaction, episode_id)
}
