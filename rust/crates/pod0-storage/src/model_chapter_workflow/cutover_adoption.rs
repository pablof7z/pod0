use std::collections::BTreeSet;

use pod0_domain::ChapterArtifactSource;
use rusqlite::Transaction;

use super::cutover::{
    LegacyModelChapterWorkflowCutoverInput, LegacyModelChapterWorkflowDisposition,
    LegacyModelChapterWorkflowEntry, MAX_CUTOVER_ENTRIES,
};
use super::ensure_replacement::replacement_record;
use super::model::{
    ModelChapterDesiredPlan, ModelChapterEnsureInput, ModelChapterWorkflowRecord,
    ModelChapterWorkflowState,
};
use super::support::{validate_blocked_plan, validate_ensure_values, validate_request};
use crate::StorageError;
use crate::chapter_store_read_selection::read_selected_chapter_artifact;

pub(super) fn validate_cutover_input(
    input: &LegacyModelChapterWorkflowCutoverInput,
) -> Result<(), StorageError> {
    if input.source_generation == 0 || input.now_ms < 0 || input.entries.len() > MAX_CUTOVER_ENTRIES
    {
        return Err(StorageError::ChapterWorkflowConflict);
    }
    let mut episodes = BTreeSet::new();
    for entry in &input.entries {
        validate_ensure_values(
            &entry.configured_model,
            input.now_ms,
            input.now_ms,
            input.max_attempts,
        )?;
        if entry.request.configured_model != entry.configured_model
            || !episodes.insert(entry.episode_id)
        {
            return Err(StorageError::ChapterWorkflowConflict);
        }
    }
    Ok(())
}

pub(super) fn migrated_record(
    transaction: &Transaction<'_>,
    cutover: &LegacyModelChapterWorkflowCutoverInput,
    entry: &LegacyModelChapterWorkflowEntry,
) -> Result<ModelChapterWorkflowRecord, StorageError> {
    validate_request(transaction, entry.episode_id, &entry.request)?;
    let desired_plan = ModelChapterDesiredPlan::Ready(Box::new(entry.request.clone()));
    let ensure = ModelChapterEnsureInput {
        episode_id: entry.episode_id,
        configured_model: entry.configured_model.clone(),
        desired_plan,
        command_id: cutover.command_id,
        cancellation_id: cutover.cancellation_id,
        issued_revision: cutover.issued_revision,
        now_ms: cutover.now_ms,
        request_deadline_ms: cutover.now_ms,
        max_attempts: cutover.max_attempts,
        force_retry_from_revision: None,
    };
    let mut record = replacement_record(None, &ensure)?;
    record.deadline_at_ms = None;
    match &entry.disposition {
        LegacyModelChapterWorkflowDisposition::Succeeded {
            artifact_id,
            content_digest,
            integrity_digest,
            selection_revision,
        } => {
            let selected = read_selected_chapter_artifact(transaction, entry.episode_id)?
                .ok_or(StorageError::ChapterWorkflowConflict)?;
            if selected.selection_revision != *selection_revision
                || selected.artifact.artifact_id != *artifact_id
                || selected.artifact.content_digest != *content_digest
                || selected.artifact.integrity_digest != *integrity_digest
                || !matches!(
                    selected.artifact.provenance.source,
                    ChapterArtifactSource::Generated | ChapterArtifactSource::PublisherEnriched
                )
            {
                return Err(StorageError::ChapterWorkflowConflict);
            }
            record.state = ModelChapterWorkflowState::Succeeded;
            record.selected_artifact_id = Some(*artifact_id);
            record.may_have_submitted = true;
            record.submission_authorized_at_ms = Some(cutover.now_ms);
        }
        LegacyModelChapterWorkflowDisposition::Ambiguous => {
            record.state = ModelChapterWorkflowState::Ambiguous;
            record.may_have_submitted = true;
            record.submission_authorized_at_ms = Some(cutover.now_ms);
            record.failure_code = Some("legacy_submission_ambiguous".into());
            record.failure_detail = Some(
                "A legacy model attempt may have reached the provider; explicit retry required."
                    .into(),
            );
        }
        LegacyModelChapterWorkflowDisposition::Blocked {
            failure_code,
            failure_detail,
            may_have_submitted,
        }
        | LegacyModelChapterWorkflowDisposition::Failed {
            failure_code,
            failure_detail,
            may_have_submitted,
        } => {
            validate_blocked_plan(failure_code, failure_detail.as_deref())?;
            record.state = if matches!(
                &entry.disposition,
                LegacyModelChapterWorkflowDisposition::Blocked { .. }
            ) {
                ModelChapterWorkflowState::Blocked
            } else {
                ModelChapterWorkflowState::Failed
            };
            record.failure_code = Some(failure_code.clone());
            record.failure_detail = failure_detail.clone();
            record.may_have_submitted = *may_have_submitted;
            record.submission_authorized_at_ms = may_have_submitted.then_some(cutover.now_ms);
        }
        LegacyModelChapterWorkflowDisposition::Cancelled { may_have_submitted } => {
            record.state = ModelChapterWorkflowState::Cancelled;
            record.failure_code = Some("legacy_cancelled".into());
            record.failure_detail = Some("The legacy model workflow was cancelled.".into());
            record.may_have_submitted = *may_have_submitted;
            record.submission_authorized_at_ms = may_have_submitted.then_some(cutover.now_ms);
        }
    }
    Ok(record)
}
