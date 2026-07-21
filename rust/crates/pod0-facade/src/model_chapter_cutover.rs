use std::collections::BTreeSet;

use pod0_application::{
    ChapterModelPlan, MODEL_CHAPTER_WORKFLOW_MAX_ATTEMPTS, PlannedChapterModelRequest,
};
use pod0_domain::CommandId;
use pod0_storage::{
    LegacyModelChapterWorkflowCutoverInput, LegacyModelChapterWorkflowDisposition,
    LegacyModelChapterWorkflowEntry, ModelChapterWorkflowAuthorityState, StorageError,
};

use crate::Pod0Facade;
use crate::model_chapter_cutover_types::{
    LegacyModelChapterCutoverCandidate, LegacyModelChapterCutoverDisposition,
    LegacyModelChapterCutoverProjection, LegacyModelChapterCutoverStage,
};
use crate::runtime_chapter_model_mapping::stored_model_request;

#[uniffi::export]
impl Pod0Facade {
    pub fn model_chapter_cutover(&self) -> LegacyModelChapterCutoverProjection {
        let state = self.state();
        let Some(store) = state.store.as_ref() else {
            return LegacyModelChapterCutoverProjection::blocked(
                StorageError::ChapterWorkflowConflict,
            );
        };
        store
            .model_chapter_workflow_authority()
            .map(LegacyModelChapterCutoverProjection::from_authority)
            .unwrap_or_else(LegacyModelChapterCutoverProjection::blocked)
    }

    pub fn stage_legacy_model_chapter_cutover(
        &self,
        source_generation: u64,
        configured_model: String,
        candidates: Vec<LegacyModelChapterCutoverCandidate>,
    ) -> LegacyModelChapterCutoverProjection {
        let result = {
            let mut state = self.state();
            let Some(store) = state.store.clone() else {
                return LegacyModelChapterCutoverProjection::blocked(
                    StorageError::ChapterWorkflowConflict,
                );
            };
            let entries = match cutover_entries(&state, &configured_model, candidates) {
                Ok(entries) => entries,
                Err(error) => return LegacyModelChapterCutoverProjection::blocked(error),
            };
            let report = store.stage_legacy_model_chapter_workflow_cutover(
                LegacyModelChapterWorkflowCutoverInput {
                    source_generation,
                    entries,
                    command_id: cutover_command_id(source_generation),
                    cancellation_id: cutover_cancellation_id(source_generation),
                    issued_revision: state.revision,
                    now_ms: state.now().value,
                    max_attempts: MODEL_CHAPTER_WORKFLOW_MAX_ATTEMPTS,
                },
            );
            if report.is_ok() {
                state.advance_revision();
            }
            report
        };
        match result {
            Ok(report) => {
                self.notify_subscribers();
                LegacyModelChapterCutoverProjection {
                    stage: match report.state {
                        ModelChapterWorkflowAuthorityState::NotStarted => {
                            LegacyModelChapterCutoverStage::NotStarted
                        }
                        ModelChapterWorkflowAuthorityState::Staged { .. } => {
                            LegacyModelChapterCutoverStage::Staged
                        }
                        ModelChapterWorkflowAuthorityState::Authoritative { .. } => {
                            LegacyModelChapterCutoverStage::Authoritative
                        }
                    },
                    source_generation: match report.state {
                        ModelChapterWorkflowAuthorityState::NotStarted => None,
                        ModelChapterWorkflowAuthorityState::Staged { source_generation }
                        | ModelChapterWorkflowAuthorityState::Authoritative { source_generation } => {
                            Some(source_generation)
                        }
                    },
                    adopted_succeeded: report.adopted_succeeded,
                    adopted_ambiguous: report.adopted_ambiguous,
                    failure: None,
                }
            }
            Err(error) => LegacyModelChapterCutoverProjection::blocked(error),
        }
    }

    pub fn commit_legacy_model_chapter_cutover(
        &self,
        source_generation: u64,
    ) -> LegacyModelChapterCutoverProjection {
        let result = {
            let mut state = self.state();
            let Some(store) = state.store.clone() else {
                return LegacyModelChapterCutoverProjection::blocked(
                    StorageError::ChapterWorkflowConflict,
                );
            };
            let result = store
                .commit_legacy_model_chapter_workflow_cutover(source_generation, state.now().value);
            if result.is_ok() {
                state.advance_revision();
            }
            result
        };
        match result {
            Ok(state) => {
                self.notify_subscribers();
                LegacyModelChapterCutoverProjection::from_authority(state)
            }
            Err(error) => LegacyModelChapterCutoverProjection::blocked(error),
        }
    }

    pub fn discard_staged_legacy_model_chapter_cutover(
        &self,
        source_generation: u64,
    ) -> LegacyModelChapterCutoverProjection {
        let result = {
            let mut state = self.state();
            let Some(store) = state.store.clone() else {
                return LegacyModelChapterCutoverProjection::blocked(
                    StorageError::ChapterWorkflowConflict,
                );
            };
            let result = store.discard_staged_legacy_model_chapter_workflow_cutover(
                source_generation,
                cutover_command_id(source_generation),
                cutover_cancellation_id(source_generation),
            );
            if result.is_ok() {
                state.advance_revision();
            }
            result
        };
        match result {
            Ok(state) => {
                self.notify_subscribers();
                LegacyModelChapterCutoverProjection::from_authority(state)
            }
            Err(error) => LegacyModelChapterCutoverProjection::blocked(error),
        }
    }
}

fn cutover_command_id(source_generation: u64) -> CommandId {
    CommandId::from_parts(0x504f_4430_4d43_5554, source_generation)
}

fn cutover_cancellation_id(source_generation: u64) -> pod0_domain::CancellationId {
    pod0_domain::CancellationId::from_parts(0x504f_4430_4d43_4341, source_generation)
}

fn cutover_entries(
    state: &crate::runtime_state::FacadeState,
    configured_model: &str,
    candidates: Vec<LegacyModelChapterCutoverCandidate>,
) -> Result<Vec<LegacyModelChapterWorkflowEntry>, StorageError> {
    let mut episodes = BTreeSet::new();
    let mut entries = Vec::new();
    for candidate in candidates {
        if candidate.input_version.is_empty() {
            return Err(StorageError::ChapterWorkflowConflict);
        }
        let request =
            match state.chapter_model_plan(candidate.episode_id, configured_model.to_owned()) {
                ChapterModelPlan::Ready { request } => request,
                ChapterModelPlan::Current { artifact_id }
                    if matches!(
                        &candidate.disposition,
                        LegacyModelChapterCutoverDisposition::Succeeded {
                            artifact_id: succeeded,
                            ..
                        } if *succeeded == artifact_id
                    ) =>
                {
                    let Some(request) = state.legacy_success_model_chapter_request(
                        candidate.episode_id,
                        configured_model.to_owned(),
                        artifact_id,
                    ) else {
                        return Err(StorageError::ChapterWorkflowConflict);
                    };
                    request
                }
                ChapterModelPlan::CoreUnavailable => {
                    return Err(StorageError::ChapterWorkflowConflict);
                }
                _ => continue,
            };
        if request.source_version != candidate.input_version {
            continue;
        }
        if !episodes.insert(candidate.episode_id) {
            return Err(StorageError::ChapterWorkflowConflict);
        }
        entries.push(LegacyModelChapterWorkflowEntry {
            episode_id: candidate.episode_id,
            configured_model: configured_model.to_owned(),
            request: map_request(configured_model, request)?,
            disposition: match candidate.disposition {
                LegacyModelChapterCutoverDisposition::Succeeded {
                    artifact_id,
                    content_digest,
                    integrity_digest,
                    selection_revision,
                } => LegacyModelChapterWorkflowDisposition::Succeeded {
                    artifact_id,
                    content_digest,
                    integrity_digest,
                    selection_revision,
                },
                LegacyModelChapterCutoverDisposition::Ambiguous => {
                    LegacyModelChapterWorkflowDisposition::Ambiguous
                }
                LegacyModelChapterCutoverDisposition::Blocked {
                    failure_code,
                    failure_detail,
                    may_have_submitted,
                } => LegacyModelChapterWorkflowDisposition::Blocked {
                    failure_code,
                    failure_detail,
                    may_have_submitted,
                },
                LegacyModelChapterCutoverDisposition::Failed {
                    failure_code,
                    failure_detail,
                    may_have_submitted,
                } => LegacyModelChapterWorkflowDisposition::Failed {
                    failure_code,
                    failure_detail,
                    may_have_submitted,
                },
                LegacyModelChapterCutoverDisposition::Cancelled { may_have_submitted } => {
                    LegacyModelChapterWorkflowDisposition::Cancelled { may_have_submitted }
                }
            },
        });
    }
    Ok(entries)
}

fn map_request(
    configured_model: &str,
    request: PlannedChapterModelRequest,
) -> Result<pod0_storage::StoredModelChapterRequest, StorageError> {
    stored_model_request(configured_model, request).ok_or(StorageError::ChapterWorkflowConflict)
}
