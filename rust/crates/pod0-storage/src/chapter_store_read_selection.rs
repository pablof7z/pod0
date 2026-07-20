use pod0_domain::EpisodeId;
use rusqlite::{Connection, OptionalExtension};

use crate::chapter_store_codec::{artifact_id, stored_u64};
use crate::{SelectedChapterArtifact, StorageError};

pub(crate) fn read_selected_chapter_artifact(
    connection: &Connection,
    episode_id: EpisodeId,
) -> Result<Option<SelectedChapterArtifact>, StorageError> {
    let selected = connection
        .query_row(
            "SELECT s.selection_revision,s.artifact_id,state.collection_revision \
             FROM pod0_chapter_selections s CROSS JOIN pod0_chapter_state state \
             WHERE s.episode_id=?1 AND state.singleton=1 \
             ORDER BY s.selection_revision DESC LIMIT 1",
            [episode_id.into_bytes().as_slice()],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Vec<u8>>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            },
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read selected chapter artifact", error))?;
    let Some((selection_revision, artifact_bytes, collection_revision)) = selected else {
        return Ok(None);
    };
    let selection_revision = stored_u64(selection_revision, "chapter selection revision")?;
    let collection_revision = stored_u64(collection_revision, "chapter collection revision")?;
    if selection_revision == 0 || selection_revision > collection_revision {
        return Err(StorageError::InvalidChapterArtifact);
    }
    let artifact_id = artifact_id(&artifact_bytes)?;
    let artifact =
        crate::chapter_store_read_artifact::read_chapter_artifact(connection, artifact_id)?
            .ok_or(StorageError::InvalidChapterArtifact)?;
    if artifact.episode_id != episode_id {
        return Err(StorageError::InvalidChapterArtifact);
    }
    Ok(Some(SelectedChapterArtifact {
        selection_revision: pod0_domain::StateRevision::new(selection_revision),
        artifact,
    }))
}
