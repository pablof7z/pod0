use pod0_domain::EpisodeId;
use rusqlite::OptionalExtension;

use super::model::ModelChapterWorkflowRecord;
use crate::{LibraryStore, StorageError};

impl LibraryStore {
    pub fn model_chapter_workflow_for_effect_intent(
        &self,
        intent_id: pod0_domain::EffectIntentId,
    ) -> Result<Option<ModelChapterWorkflowRecord>, StorageError> {
        self.read(|connection| {
            let episode: Option<Vec<u8>> = connection
                .query_row(
                    "SELECT episode_id FROM pod0_effect_intents WHERE intent_id=?1
                     AND effect_kind_code=4 AND subject_code=2",
                    [intent_id.into_bytes().as_slice()],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|error| {
                    StorageError::sqlite("read model chapter effect workflow", error)
                })?;
            episode
                .map(|bytes| {
                    let episode_id = EpisodeId::from_bytes(
                        bytes
                            .try_into()
                            .map_err(|_| StorageError::InvalidActivity)?,
                    );
                    super::read::read_workflow(connection, episode_id)
                })
                .transpose()
                .map(Option::flatten)
        })
    }
}
