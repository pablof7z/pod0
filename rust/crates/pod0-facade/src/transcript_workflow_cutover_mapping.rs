use std::collections::BTreeSet;

use pod0_application::{
    transcript_attempt_id, transcript_evidence_input_version, transcript_submission_fence_id,
};
use pod0_domain::EpisodeId;
use pod0_storage::{
    LegacyTranscriptWorkflowBackupRow as StoredRow,
    LegacyTranscriptWorkflowCandidate as StoredCandidate,
    LegacyTranscriptWorkflowDisposition as StoredDisposition,
    LegacyTranscriptWorkflowRowClassification as StoredClassification, PreparedTranscriptAttempt,
    StorageError,
};

use crate::runtime_state::FacadeState;
use crate::runtime_transcript_workflow_mapping::{request_id, stored_request};
use crate::transcript_workflow_cutover_types::{
    LegacyTranscriptWorkflowCutoverCandidate, LegacyTranscriptWorkflowCutoverDisposition,
};

pub(super) fn cutover_candidates(
    state: &FacadeState,
    rows: &[StoredRow],
    candidates: Vec<LegacyTranscriptWorkflowCutoverCandidate>,
    now_ms: i64,
) -> Result<Vec<StoredCandidate>, StorageError> {
    let mut episodes = BTreeSet::new();
    candidates
        .into_iter()
        .map(|candidate| {
            if !episodes.insert(candidate.episode_id)
                || !rows.iter().any(|row| {
                    row.episode_id == candidate.episode_id
                        && row.classification == expected_classification(&candidate.disposition)
                })
            {
                return Err(StorageError::TranscriptWorkflowConflict);
            }
            stored_candidate(state, candidate, now_ms)
        })
        .collect()
}

fn stored_candidate(
    state: &FacadeState,
    candidate: LegacyTranscriptWorkflowCutoverCandidate,
    now_ms: i64,
) -> Result<StoredCandidate, StorageError> {
    let (request, expected_selection_revision) = state
        .legacy_transcript_workflow_request(
            candidate.episode_id,
            candidate.origin,
            candidate.configuration.clone(),
        )
        .ok_or(StorageError::TranscriptWorkflowConflict)?;
    if request.source_revision != candidate.source_revision {
        return Err(StorageError::TranscriptWorkflowConflict);
    }
    let attempt = disposition_attempt(&candidate.disposition);
    let prepared_attempt = match attempt {
        Some(attempt) => {
            let attempt_id = transcript_attempt_id(request.workflow_id, attempt)
                .ok_or(StorageError::TranscriptWorkflowConflict)?;
            Some(PreparedTranscriptAttempt {
                attempt,
                attempt_id,
                submission_fence_id: transcript_submission_fence_id(attempt_id),
            })
        }
        None => None,
    };
    let host_request_id =
        prepared_attempt.map(|attempt| request_id(request.workflow_id, attempt.attempt, false));
    let deadline_at_ms = matches!(
        candidate.disposition,
        LegacyTranscriptWorkflowCutoverDisposition::Restart { .. }
    )
    .then(|| {
        now_ms.saturating_add(pod0_application::TRANSCRIPT_HOST_REQUEST_DEADLINE_MILLISECONDS)
    });
    let disposition = stored_disposition(state, &candidate)?;
    Ok(StoredCandidate {
        episode_id: candidate.episode_id,
        request: stored_request(request),
        request_id: host_request_id,
        prepared_attempt,
        deadline_at_ms,
        expected_selection_revision,
        disposition,
    })
}

fn stored_disposition(
    state: &FacadeState,
    candidate: &LegacyTranscriptWorkflowCutoverCandidate,
) -> Result<StoredDisposition, StorageError> {
    Ok(match &candidate.disposition {
        LegacyTranscriptWorkflowCutoverDisposition::Restart { .. } => StoredDisposition::Restart,
        LegacyTranscriptWorkflowCutoverDisposition::RecoverProvider {
            external_operation_id,
            provider_status,
            ..
        } => StoredDisposition::RecoverProvider {
            external_operation_id: external_operation_id.clone(),
            provider_status: provider_status.clone(),
        },
        LegacyTranscriptWorkflowCutoverDisposition::Ambiguous { .. } => {
            StoredDisposition::Ambiguous
        }
        LegacyTranscriptWorkflowCutoverDisposition::Blocked {
            failure_code,
            failure_detail,
            may_have_submitted,
            ..
        } => StoredDisposition::Blocked {
            failure_code: failure_code.clone(),
            failure_detail: failure_detail.clone(),
            may_have_submitted: *may_have_submitted,
        },
        LegacyTranscriptWorkflowCutoverDisposition::Failed {
            failure_code,
            failure_detail,
            may_have_submitted,
            ..
        } => StoredDisposition::Failed {
            failure_code: failure_code.clone(),
            failure_detail: failure_detail.clone(),
            may_have_submitted: *may_have_submitted,
        },
        LegacyTranscriptWorkflowCutoverDisposition::Cancelled {
            may_have_submitted, ..
        } => StoredDisposition::Cancelled {
            may_have_submitted: *may_have_submitted,
        },
        LegacyTranscriptWorkflowCutoverDisposition::Succeeded { .. } => selected_disposition(
            state,
            candidate.episode_id,
            &candidate.source_revision,
            None,
            false,
        )?,
        LegacyTranscriptWorkflowCutoverDisposition::IndexPending {
            evidence_input_version,
        } => selected_disposition(
            state,
            candidate.episode_id,
            &candidate.source_revision,
            Some(evidence_input_version),
            false,
        )?,
        LegacyTranscriptWorkflowCutoverDisposition::IndexSucceeded {
            evidence_input_version,
        } => selected_disposition(
            state,
            candidate.episode_id,
            &candidate.source_revision,
            Some(evidence_input_version),
            true,
        )?,
    })
}

fn selected_disposition(
    state: &FacadeState,
    episode_id: EpisodeId,
    source_revision: &str,
    evidence_input_version: Option<&String>,
    evidence_complete: bool,
) -> Result<StoredDisposition, StorageError> {
    let selected = state
        .transcript_store
        .as_ref()
        .ok_or(StorageError::TranscriptWorkflowConflict)?
        .selected_summary(episode_id)?
        .filter(|value| value.source_revision == source_revision)
        .ok_or(StorageError::TranscriptWorkflowConflict)?;
    if let Some(input) = evidence_input_version {
        let embedding_space = state
            .recall_configuration
            .embedding_space_id
            .into_bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let expected = transcript_evidence_input_version(
            selected.transcript_version_id,
            selected.transcript_content_digest,
            &embedding_space,
        )
        .ok_or(StorageError::TranscriptWorkflowConflict)?;
        if *input != expected {
            return Err(StorageError::TranscriptWorkflowConflict);
        }
        if evidence_complete {
            let evidence_matches = state
                .evidence_store
                .as_ref()
                .ok_or(StorageError::TranscriptWorkflowConflict)?
                .selected_generation(episode_id)?
                .is_some_and(|value| value.transcript_version_id == selected.transcript_version_id);
            if !evidence_matches {
                return Err(StorageError::TranscriptWorkflowConflict);
            }
        }
    }
    let identity = (
        selected.artifact_id,
        selected.transcript_version_id,
        selected.transcript_content_digest,
        selected.selection_revision,
    );
    Ok(match (evidence_input_version, evidence_complete) {
        (None, _) => StoredDisposition::Succeeded {
            artifact_id: identity.0,
            transcript_version_id: identity.1,
            content_digest: identity.2,
            selection_revision: identity.3,
        },
        (Some(input), false) => StoredDisposition::IndexPending {
            artifact_id: identity.0,
            transcript_version_id: identity.1,
            content_digest: identity.2,
            selection_revision: identity.3,
            evidence_input_version: input.clone(),
        },
        (Some(input), true) => StoredDisposition::IndexSucceeded {
            artifact_id: identity.0,
            transcript_version_id: identity.1,
            content_digest: identity.2,
            selection_revision: identity.3,
            evidence_input_version: input.clone(),
        },
    })
}

fn disposition_attempt(value: &LegacyTranscriptWorkflowCutoverDisposition) -> Option<u16> {
    match value {
        LegacyTranscriptWorkflowCutoverDisposition::Restart { attempt }
        | LegacyTranscriptWorkflowCutoverDisposition::RecoverProvider { attempt, .. }
        | LegacyTranscriptWorkflowCutoverDisposition::Ambiguous { attempt } => Some(*attempt),
        LegacyTranscriptWorkflowCutoverDisposition::Blocked { attempt, .. }
        | LegacyTranscriptWorkflowCutoverDisposition::Failed { attempt, .. }
        | LegacyTranscriptWorkflowCutoverDisposition::Cancelled { attempt, .. }
        | LegacyTranscriptWorkflowCutoverDisposition::Succeeded { attempt } => *attempt,
        LegacyTranscriptWorkflowCutoverDisposition::IndexPending { .. }
        | LegacyTranscriptWorkflowCutoverDisposition::IndexSucceeded { .. } => None,
    }
}

fn expected_classification(
    value: &LegacyTranscriptWorkflowCutoverDisposition,
) -> StoredClassification {
    match value {
        LegacyTranscriptWorkflowCutoverDisposition::Restart { .. } => StoredClassification::Restart,
        LegacyTranscriptWorkflowCutoverDisposition::RecoverProvider { .. } => {
            StoredClassification::RecoverProvider
        }
        LegacyTranscriptWorkflowCutoverDisposition::Ambiguous { .. } => {
            StoredClassification::Ambiguous
        }
        LegacyTranscriptWorkflowCutoverDisposition::Blocked { .. } => StoredClassification::Blocked,
        LegacyTranscriptWorkflowCutoverDisposition::Failed { .. } => StoredClassification::Failed,
        LegacyTranscriptWorkflowCutoverDisposition::Cancelled { .. } => {
            StoredClassification::Cancelled
        }
        LegacyTranscriptWorkflowCutoverDisposition::Succeeded { .. } => {
            StoredClassification::Succeeded
        }
        LegacyTranscriptWorkflowCutoverDisposition::IndexPending { .. } => {
            StoredClassification::IndexPending
        }
        LegacyTranscriptWorkflowCutoverDisposition::IndexSucceeded { .. } => {
            StoredClassification::IndexSucceeded
        }
    }
}
