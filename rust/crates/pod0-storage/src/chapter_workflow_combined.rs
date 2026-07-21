use pod0_domain::EpisodeId;
use rusqlite::params;

use crate::{
    LibraryStore, ModelChapterWorkflowRecord, PublisherChapterWorkflowRecord, StorageError,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChapterWorkflowRecord {
    Publisher(Box<PublisherChapterWorkflowRecord>),
    Model(Box<ModelChapterWorkflowRecord>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChapterWorkflowPage {
    pub items: Vec<ChapterWorkflowRecord>,
    pub has_more: bool,
}

impl LibraryStore {
    pub fn chapter_workflow_page(
        &self,
        episode_id: Option<EpisodeId>,
        offset: u32,
        max_items: u16,
    ) -> Result<ChapterWorkflowPage, StorageError> {
        self.read(|connection| {
            let limit = usize::from(max_items.max(1));
            let fetch = i64::try_from(limit + 1).expect("bounded workflow page limit");
            let mut statement = connection
                .prepare(
                    "SELECT workflow_kind,episode_id FROM (\
                     SELECT 0 AS workflow_kind,episode_id,updated_at_ms FROM \
                     pod0_publisher_chapter_workflows WHERE state!='source_absent' \
                     AND (?1 IS NULL OR episode_id=?1) UNION ALL \
                     SELECT 1 AS workflow_kind,episode_id,updated_at_ms FROM \
                     pod0_model_chapter_workflows WHERE (?1 IS NULL OR episode_id=?1)) \
                     ORDER BY updated_at_ms DESC,workflow_kind,episode_id LIMIT ?2 OFFSET ?3",
                )
                .map_err(|error| {
                    StorageError::sqlite("prepare combined chapter workflows", error)
                })?;
            let rows = statement
                .query_map(
                    params![
                        episode_id.map(|id| id.into_bytes().to_vec()),
                        fetch,
                        i64::from(offset)
                    ],
                    |row| Ok((row.get::<_, i64>(0)?, row.get::<_, Vec<u8>>(1)?)),
                )
                .map_err(|error| StorageError::sqlite("query combined chapter workflows", error))?;
            let keys = rows.collect::<Result<Vec<_>, _>>().map_err(|error| {
                StorageError::sqlite("decode combined chapter workflows", error)
            })?;
            let has_more = keys.len() > limit;
            let mut items = Vec::with_capacity(limit.min(keys.len()));
            for (kind, bytes) in keys.into_iter().take(limit) {
                let episode_id = EpisodeId::from_bytes(
                    bytes
                        .try_into()
                        .map_err(|_| StorageError::ChapterWorkflowConflict)?,
                );
                let item = match kind {
                    0 => crate::chapter_workflow_store_read::read_workflow(connection, episode_id)?
                        .map(Box::new)
                        .map(ChapterWorkflowRecord::Publisher),
                    1 => {
                        crate::model_chapter_workflow::read::read_workflow(connection, episode_id)?
                            .map(Box::new)
                            .map(ChapterWorkflowRecord::Model)
                    }
                    _ => return Err(StorageError::ChapterWorkflowConflict),
                }
                .ok_or(StorageError::ChapterWorkflowNotFound)?;
                items.push(item);
            }
            Ok(ChapterWorkflowPage { items, has_more })
        })
    }
}
