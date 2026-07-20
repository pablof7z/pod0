use pod0_application::{
    ChapterArtifactProjection, ChapterProjectionScope, CoreFailureCode, MAX_OPERATION_ITEMS,
};
use pod0_domain::EpisodeId;

use crate::runtime_state::{FacadeState, failure};
use crate::runtime_storage_commands::storage_failure;

impl FacadeState {
    pub(super) fn chapter_projection(
        &self,
        episode_id: EpisodeId,
        scope: ChapterProjectionScope,
        offset: u32,
        max_items: u16,
    ) -> ChapterArtifactProjection {
        let mut projection = empty_projection(scope, &self.operations);
        let Some(store) = &self.store else {
            projection.failure = Some(failure(CoreFailureCode::StorageUnavailable));
            return projection;
        };
        match store.selected_chapter_artifact(episode_id) {
            Ok(Some(selected)) => {
                projection = pod0_application::project_chapter_artifact(
                    &selected.artifact,
                    selected.selection_revision,
                    scope,
                    usize::try_from(offset).unwrap_or(usize::MAX),
                    usize::from(max_items),
                );
                projection.operations = self
                    .operations
                    .iter()
                    .take(MAX_OPERATION_ITEMS)
                    .cloned()
                    .collect();
            }
            Ok(None) => {}
            Err(error) => projection.failure = Some(failure(storage_failure(error))),
        }
        projection
    }
}

fn empty_projection(
    scope: ChapterProjectionScope,
    operations: &[pod0_application::OperationProjection],
) -> ChapterArtifactProjection {
    ChapterArtifactProjection {
        scope,
        summary: None,
        chapters: Vec::new(),
        ad_spans: Vec::new(),
        operations: operations
            .iter()
            .take(MAX_OPERATION_ITEMS)
            .cloned()
            .collect(),
        failure: None,
        has_more: false,
    }
}
