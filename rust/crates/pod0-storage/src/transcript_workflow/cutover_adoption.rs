use pod0_domain::{ContentDigest, StateRevision};
use rusqlite::Transaction;

use super::cutover::{
    LegacyTranscriptWorkflowCandidate, LegacyTranscriptWorkflowCutoverInput,
    LegacyTranscriptWorkflowDisposition,
};
use super::cutover_adoption_state::{adopt_attempt_state, adopt_evidence_state};
use super::model::{StoredTranscriptWorkflowStage, TranscriptWorkflowRecord};
use super::persist::{insert_prepared_attempt, persist_workflow};
use crate::StorageError;
use crate::transcript_store_read_rows::read_summary;

pub(super) fn adopt_candidate(
    transaction: &Transaction<'_>,
    cutover: &LegacyTranscriptWorkflowCutoverInput,
    candidate: &LegacyTranscriptWorkflowCandidate,
) -> Result<(), StorageError> {
    let parent: bool = transaction
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM pod0_episodes WHERE episode_id=?1)",
            [candidate.episode_id.into_bytes().as_slice()],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("validate transcript workflow episode", error))?;
    if !parent {
        return Err(StorageError::TranscriptWorkflowConflict);
    }
    let mut record = base_record(cutover, candidate);
    apply_disposition(transaction, &mut record, &candidate.disposition)?;
    persist_workflow(transaction, &record)?;
    insert_prepared_attempt(transaction, &record)?;
    adopt_attempt_state(transaction, &record)?;
    adopt_evidence_state(transaction, &record)
}

fn base_record(
    cutover: &LegacyTranscriptWorkflowCutoverInput,
    candidate: &LegacyTranscriptWorkflowCandidate,
) -> TranscriptWorkflowRecord {
    let attempt = candidate.prepared_attempt;
    TranscriptWorkflowRecord {
        episode_id: candidate.episode_id,
        request: candidate.request.clone(),
        stage: StoredTranscriptWorkflowStage::Blocked,
        workflow_revision: StateRevision::new(1),
        attempt: attempt.map_or(0, |value| value.attempt),
        max_attempts: cutover.max_attempts,
        attempt_id: attempt.map(|value| value.attempt_id),
        submission_fence_id: attempt.map(|value| value.submission_fence_id),
        command_id: cutover.command_id,
        cancellation_id: cutover.cancellation_id,
        request_id: candidate.request_id,
        issued_revision: cutover.issued_revision,
        deadline_at_ms: candidate.deadline_at_ms,
        not_before_ms: None,
        submission_authorized_at_ms: None,
        external_operation_id: None,
        provider_status: None,
        completion_artifact_id: None,
        committed_artifact_id: None,
        committed_transcript_version_id: None,
        committed_content_digest: None,
        expected_selection_revision: candidate.expected_selection_revision,
        resulting_selection_revision: None,
        evidence_input_version: None,
        failure_code: None,
        failure_detail: None,
        failure_retryable: false,
        may_have_submitted: false,
        source_generation: Some(cutover.source_generation),
        created_at_ms: cutover.now_ms,
        updated_at_ms: cutover.now_ms,
    }
}

fn apply_disposition(
    transaction: &Transaction<'_>,
    record: &mut TranscriptWorkflowRecord,
    disposition: &LegacyTranscriptWorkflowDisposition,
) -> Result<(), StorageError> {
    match disposition {
        LegacyTranscriptWorkflowDisposition::Restart => {
            record.stage = if record.request.publisher_first {
                StoredTranscriptWorkflowStage::PublisherRequested
            } else {
                StoredTranscriptWorkflowStage::Requested
            };
        }
        LegacyTranscriptWorkflowDisposition::RecoverProvider {
            external_operation_id,
            provider_status,
        } => {
            record.stage = StoredTranscriptWorkflowStage::ProviderAccepted;
            record.external_operation_id = Some(external_operation_id.clone());
            record.provider_status = provider_status.clone();
            record.submission_authorized_at_ms = Some(record.updated_at_ms);
            record.may_have_submitted = true;
        }
        LegacyTranscriptWorkflowDisposition::Ambiguous => {
            record.stage = StoredTranscriptWorkflowStage::Blocked;
            record.failure_code = Some("ambiguous_submission".into());
            record.may_have_submitted = true;
            record.submission_authorized_at_ms = Some(record.updated_at_ms);
        }
        LegacyTranscriptWorkflowDisposition::Blocked {
            failure_code,
            failure_detail,
            may_have_submitted,
        } => set_failure(
            record,
            StoredTranscriptWorkflowStage::Blocked,
            failure_code,
            failure_detail,
            *may_have_submitted,
        ),
        LegacyTranscriptWorkflowDisposition::Failed {
            failure_code,
            failure_detail,
            may_have_submitted,
        } => set_failure(
            record,
            StoredTranscriptWorkflowStage::Failed,
            failure_code,
            failure_detail,
            *may_have_submitted,
        ),
        LegacyTranscriptWorkflowDisposition::Cancelled { may_have_submitted } => {
            record.stage = StoredTranscriptWorkflowStage::Cancelled;
            record.failure_code = Some("cancelled".into());
            record.may_have_submitted = *may_have_submitted;
            record.submission_authorized_at_ms = may_have_submitted.then_some(record.updated_at_ms);
        }
        LegacyTranscriptWorkflowDisposition::Succeeded {
            artifact_id,
            transcript_version_id,
            content_digest,
            selection_revision,
        } => set_success(
            transaction,
            record,
            (
                *artifact_id,
                *transcript_version_id,
                *content_digest,
                *selection_revision,
            ),
            None,
            false,
        )?,
        LegacyTranscriptWorkflowDisposition::IndexPending {
            artifact_id,
            transcript_version_id,
            content_digest,
            selection_revision,
            evidence_input_version,
        } => set_success(
            transaction,
            record,
            (
                *artifact_id,
                *transcript_version_id,
                *content_digest,
                *selection_revision,
            ),
            Some(evidence_input_version.clone()),
            false,
        )?,
        LegacyTranscriptWorkflowDisposition::IndexSucceeded {
            artifact_id,
            transcript_version_id,
            content_digest,
            selection_revision,
            evidence_input_version,
        } => set_success(
            transaction,
            record,
            (
                *artifact_id,
                *transcript_version_id,
                *content_digest,
                *selection_revision,
            ),
            Some(evidence_input_version.clone()),
            true,
        )?,
    }
    Ok(())
}

fn set_failure(
    record: &mut TranscriptWorkflowRecord,
    stage: StoredTranscriptWorkflowStage,
    code: &str,
    detail: &Option<String>,
    may_submit: bool,
) {
    record.stage = stage;
    record.failure_code = Some(code.to_owned());
    record.failure_detail = detail.clone();
    record.may_have_submitted = may_submit;
    record.submission_authorized_at_ms = may_submit.then_some(record.updated_at_ms);
}

type SelectedIdentity = (
    pod0_domain::TranscriptArtifactId,
    pod0_domain::TranscriptVersionId,
    ContentDigest,
    StateRevision,
);

fn set_success(
    transaction: &Transaction<'_>,
    record: &mut TranscriptWorkflowRecord,
    expected: SelectedIdentity,
    evidence: Option<String>,
    evidence_complete: bool,
) -> Result<(), StorageError> {
    let selected = read_summary(transaction, record.episode_id)?
        .ok_or(StorageError::TranscriptWorkflowConflict)?;
    if (
        selected.artifact_id,
        selected.transcript_version_id,
        selected.transcript_content_digest,
        selected.selection_revision,
    ) != expected
    {
        return Err(StorageError::TranscriptWorkflowConflict);
    }
    record.stage = if evidence_complete {
        StoredTranscriptWorkflowStage::Succeeded
    } else if evidence.is_some() {
        StoredTranscriptWorkflowStage::EvidenceRequested
    } else {
        StoredTranscriptWorkflowStage::TranscriptCommitted
    };
    record.completion_artifact_id = Some(expected.0);
    record.committed_artifact_id = Some(expected.0);
    record.committed_transcript_version_id = Some(expected.1);
    record.committed_content_digest = Some(expected.2);
    record.resulting_selection_revision = Some(expected.3);
    record.evidence_input_version = evidence;
    record.may_have_submitted = record.attempt_id.is_some();
    record.submission_authorized_at_ms = record.may_have_submitted.then_some(record.updated_at_ms);
    Ok(())
}
