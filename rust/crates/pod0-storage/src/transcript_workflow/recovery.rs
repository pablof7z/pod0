use rusqlite::params;

use super::authority::require_authoritative;
use super::model::{
    MAX_TRANSCRIPT_WORKFLOW_PAGE_ITEMS, StoredTranscriptWorkflowStage, TranscriptWorkflowRecord,
    TranscriptWorkflowRecoveryReport,
};
use super::persist::persist_workflow;
use super::read::{WORKFLOW_COLUMNS, decode_row};
use super::support::next_revision;
use crate::{LibraryStore, StorageError};

impl LibraryStore {
    pub fn recover_transcript_workflows(
        &self,
        now_ms: i64,
        maximum_count: u16,
    ) -> Result<TranscriptWorkflowRecoveryReport, StorageError> {
        if now_ms < 0 {
            return Err(StorageError::TranscriptWorkflowConflict);
        }
        let limit = maximum_count.clamp(1, MAX_TRANSCRIPT_WORKFLOW_PAGE_ITEMS);
        self.write(|transaction| {
            require_authoritative(transaction)?;
            let mut statement = transaction.prepare(&format!(
                "SELECT {WORKFLOW_COLUMNS} FROM pod0_transcript_workflows WHERE stage IN(
                 'requested','publisher_requested','submission_authorized','provider_accepted',
                 'completion_observed','transcript_committed','evidence_requested','retry_scheduled')
                 ORDER BY updated_at_ms,episode_id LIMIT ?1"
            )).map_err(|error| StorageError::sqlite("prepare transcript recovery", error))?;
            let rows = statement.query_map([i64::from(limit)+1], decode_row)
                .map_err(|error| StorageError::sqlite("query transcript recovery", error))?;
            let mut records = rows.collect::<Result<Vec<_>,_>>()
                .map_err(|_| StorageError::TranscriptWorkflowConflict)?;
            let has_more = records.len() > usize::from(limit);
            records.truncate(usize::from(limit));
            drop(statement);
            let mut report = TranscriptWorkflowRecoveryReport {
                dispatchable_requests: Vec::new(), ambiguous_requests: Vec::new(),
                provider_recoveries: Vec::new(), completions_to_commit: Vec::new(),
                evidence_requests: Vec::new(), has_more,
            };
            for mut record in records {
                classify_recovery(transaction, &mut record, now_ms, &mut report)?;
            }
            Ok(report)
        })
    }
}

fn classify_recovery(
    transaction: &rusqlite::Transaction<'_>,
    record: &mut TranscriptWorkflowRecord,
    now_ms: i64,
    report: &mut TranscriptWorkflowRecoveryReport,
) -> Result<(), StorageError> {
    match record.stage {
        StoredTranscriptWorkflowStage::Requested
        | StoredTranscriptWorkflowStage::PublisherRequested => {
            if record
                .deadline_at_ms
                .is_some_and(|deadline| deadline >= now_ms)
            {
                push_request(&mut report.dispatchable_requests, record)?;
            }
        }
        StoredTranscriptWorkflowStage::RetryScheduled => {
            if record.not_before_ms.is_some_and(|due| due <= now_ms) {
                push_request(&mut report.dispatchable_requests, record)?;
            }
        }
        StoredTranscriptWorkflowStage::SubmissionAuthorized => {
            record.stage = StoredTranscriptWorkflowStage::Blocked;
            record.workflow_revision = next_revision(record.workflow_revision)?;
            record.failure_code = Some("ambiguous_submission".to_owned());
            record.failure_detail = Some(
                "Submission was authorized before termination; provider recovery or explicit review is required."
                    .to_owned(),
            );
            record.failure_retryable = false;
            record.may_have_submitted = true;
            record.deadline_at_ms = None;
            record.updated_at_ms = now_ms.max(record.updated_at_ms);
            persist_workflow(transaction, record)?;
            if let Some(attempt_id) = record.attempt_id {
                transaction.execute(
                    "UPDATE pod0_transcript_attempts SET state='ambiguous',failure_code='ambiguous_submission',
                     may_have_submitted=1,updated_at_ms=?1 WHERE attempt_id=?2 AND state='authorized'",
                    params![record.updated_at_ms,attempt_id.into_bytes().as_slice()],
                ).map_err(|error| StorageError::sqlite("fence ambiguous transcript attempt", error))?;
            }
            push_request(&mut report.ambiguous_requests, record)?;
        }
        StoredTranscriptWorkflowStage::ProviderAccepted => {
            push_request(&mut report.provider_recoveries, record)?
        }
        StoredTranscriptWorkflowStage::CompletionObserved => {
            push_request(&mut report.completions_to_commit, record)?
        }
        StoredTranscriptWorkflowStage::TranscriptCommitted
        | StoredTranscriptWorkflowStage::EvidenceRequested => {
            report.evidence_requests.push(record.request.workflow_id);
        }
        _ => {}
    }
    Ok(())
}

fn push_request(
    output: &mut Vec<pod0_domain::HostRequestId>,
    record: &TranscriptWorkflowRecord,
) -> Result<(), StorageError> {
    output.push(
        record
            .request_id
            .ok_or(StorageError::TranscriptWorkflowConflict)?,
    );
    Ok(())
}
