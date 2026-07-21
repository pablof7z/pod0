use pod0_domain::{EpisodeId, StateRevision};

use super::inputs::ModelChapterRecoveryReport;
use super::model::{ModelChapterWorkflowRecord, ModelChapterWorkflowState};
use super::persist::persist_workflow;
use super::read::{read_recoverable_workflows, read_workflow};
use crate::{LibraryStore, StorageError};

impl LibraryStore {
    pub fn recover_model_chapter_workflows(
        &self,
        max_items: u16,
        observed_at_ms: i64,
    ) -> Result<ModelChapterRecoveryReport, StorageError> {
        if observed_at_ms < 0 || max_items == 0 {
            return Err(StorageError::ChapterWorkflowConflict);
        }
        self.write(|transaction| {
            let requested = max_items.saturating_add(1);
            let mut records = read_recoverable_workflows(transaction, requested)?;
            let has_more = records.len() > usize::from(max_items);
            records.truncate(usize::from(max_items));
            let mut report = ModelChapterRecoveryReport {
                ambiguous_requests: Vec::new(),
                resumable_provider_requests: Vec::new(),
                staged_completions: Vec::new(),
                has_more,
            };
            for mut record in records {
                let request_id = record
                    .request_id
                    .ok_or(StorageError::ChapterWorkflowConflict)?;
                match record.state {
                    ModelChapterWorkflowState::SubmissionAuthorized => {
                        record.state = ModelChapterWorkflowState::Ambiguous;
                        record.workflow_revision = next_revision(record.workflow_revision)?;
                        record.failure_code = Some("ambiguous_submission".to_owned());
                        record.failure_detail = Some(
                            "process ended after submission authorization without durable provider evidence"
                                .to_owned(),
                        );
                        record.deadline_at_ms = None;
                        record.not_before_ms = None;
                        record.may_have_submitted = true;
                        record.updated_at_ms = observed_at_ms;
                        persist_workflow(transaction, &record)?;
                        report.ambiguous_requests.push(request_id);
                    }
                    ModelChapterWorkflowState::ProviderAccepted => {
                        report.resumable_provider_requests.push(request_id);
                    }
                    ModelChapterWorkflowState::CompletionObserved => {
                        report.staged_completions.push(request_id);
                    }
                    _ => return Err(StorageError::ChapterWorkflowConflict),
                }
            }
            Ok(report)
        })
    }

    pub fn cancel_model_chapter_workflow(
        &self,
        episode_id: EpisodeId,
        expected_revision: StateRevision,
        observed_at_ms: i64,
    ) -> Result<ModelChapterWorkflowRecord, StorageError> {
        if observed_at_ms < 0 {
            return Err(StorageError::ChapterWorkflowConflict);
        }
        self.write(|transaction| {
            let mut record = read_workflow(transaction, episode_id)?
                .ok_or(StorageError::ChapterWorkflowNotFound)?;
            if record.workflow_revision != expected_revision
                || record.state.protects_active_attempt()
                || !matches!(
                    record.state,
                    ModelChapterWorkflowState::AwaitingTranscript
                        | ModelChapterWorkflowState::AwaitingPublisher
                        | ModelChapterWorkflowState::Requested
                        | ModelChapterWorkflowState::RetryScheduled
                        | ModelChapterWorkflowState::Ambiguous
                        | ModelChapterWorkflowState::Blocked
                        | ModelChapterWorkflowState::Failed
                )
            {
                return Err(StorageError::ChapterWorkflowConflict);
            }
            record.state = ModelChapterWorkflowState::Cancelled;
            record.workflow_revision = next_revision(record.workflow_revision)?;
            record.deadline_at_ms = None;
            record.not_before_ms = None;
            record.failure_code = Some("cancelled".to_owned());
            record.failure_detail = None;
            record.updated_at_ms = observed_at_ms;
            persist_workflow(transaction, &record)?;
            read_workflow(transaction, episode_id)?.ok_or(StorageError::ChapterWorkflowNotFound)
        })
    }
}

fn next_revision(current: StateRevision) -> Result<StateRevision, StorageError> {
    current
        .value
        .checked_add(1)
        .map(StateRevision::new)
        .ok_or(StorageError::ChapterWorkflowConflict)
}
