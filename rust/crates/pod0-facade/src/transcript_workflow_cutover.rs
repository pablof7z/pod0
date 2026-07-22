use pod0_domain::ContentDigest;
use pod0_storage::{
    LegacyTranscriptWorkflowCutoverInput, StorageError, TranscriptWorkflowAuthorityState,
};

use crate::Pod0Facade;
use crate::transcript_workflow_cutover_mapping::cutover_candidates;
use crate::transcript_workflow_cutover_rows::{source_generation, stored_rows};
use crate::transcript_workflow_cutover_types::{
    LegacyTranscriptWorkflowBackupRow, LegacyTranscriptWorkflowCutoverCandidate,
    LegacyTranscriptWorkflowCutoverProjection,
};

#[uniffi::export]
impl Pod0Facade {
    pub fn transcript_workflow_cutover(&self) -> LegacyTranscriptWorkflowCutoverProjection {
        let state = self.state();
        let Some(store) = state.store.as_ref() else {
            return LegacyTranscriptWorkflowCutoverProjection::blocked(
                StorageError::TranscriptWorkflowConflict,
            );
        };
        match store.transcript_workflow_cutover_report() {
            Ok(Some(report)) => LegacyTranscriptWorkflowCutoverProjection::from_report(report),
            Ok(None) => LegacyTranscriptWorkflowCutoverProjection::not_started(),
            Err(error) => LegacyTranscriptWorkflowCutoverProjection::blocked(error),
        }
    }

    pub fn stage_legacy_transcript_workflow_cutover(
        &self,
        backup_digest: ContentDigest,
        backup_byte_count: u64,
        rows: Vec<LegacyTranscriptWorkflowBackupRow>,
        candidates: Vec<LegacyTranscriptWorkflowCutoverCandidate>,
    ) -> LegacyTranscriptWorkflowCutoverProjection {
        let result = {
            let mut state = self.state();
            let Some(store) = state.store.clone() else {
                return LegacyTranscriptWorkflowCutoverProjection::blocked(
                    StorageError::TranscriptWorkflowConflict,
                );
            };
            let rows = match stored_rows(rows) {
                Ok(rows) => rows,
                Err(error) => return LegacyTranscriptWorkflowCutoverProjection::blocked(error),
            };
            let fingerprint = pod0_storage::transcript_workflow_source_fingerprint(&rows);
            let generation = source_generation(fingerprint);
            let now_ms = state.now().value;
            let candidates = match cutover_candidates(&state, &rows, candidates, now_ms) {
                Ok(candidates) => candidates,
                Err(error) => return LegacyTranscriptWorkflowCutoverProjection::blocked(error),
            };
            let result = store.stage_legacy_transcript_workflow_cutover(
                LegacyTranscriptWorkflowCutoverInput {
                    source_generation: generation,
                    source_fingerprint: fingerprint,
                    backup_digest,
                    backup_byte_count,
                    rows,
                    candidates,
                    command_id: cutover_command_id(generation),
                    cancellation_id: cutover_cancellation_id(generation),
                    issued_revision: state.revision,
                    max_attempts: pod0_application::TRANSCRIPT_WORKFLOW_MAX_ATTEMPTS,
                    now_ms,
                },
            );
            if result.is_ok() {
                state.advance_revision();
            }
            result
        };
        self.cutover_result(result)
    }

    pub fn verify_legacy_transcript_workflow_cutover(
        &self,
        source_generation: u64,
    ) -> LegacyTranscriptWorkflowCutoverProjection {
        let result = {
            let mut state = self.state();
            let Some(store) = state.store.clone() else {
                return LegacyTranscriptWorkflowCutoverProjection::blocked(
                    StorageError::TranscriptWorkflowConflict,
                );
            };
            let report = match store.transcript_workflow_cutover_report() {
                Ok(Some(report)) if report_source_generation(report.state) == source_generation => {
                    report
                }
                Ok(_) => {
                    return LegacyTranscriptWorkflowCutoverProjection::blocked(
                        StorageError::TranscriptWorkflowConflict,
                    );
                }
                Err(error) => {
                    return LegacyTranscriptWorkflowCutoverProjection::blocked(error);
                }
            };
            let result = store.verify_legacy_transcript_workflow_cutover(
                source_generation,
                report.source_fingerprint,
                state.now().value,
            );
            if result.is_ok() {
                state.advance_revision();
            }
            result
        };
        self.cutover_result(result)
    }

    pub fn commit_legacy_transcript_workflow_cutover(
        &self,
        source_generation: u64,
    ) -> LegacyTranscriptWorkflowCutoverProjection {
        self.cutover_result(self.commit_transcript_workflow_cutover(source_generation))
    }

    pub fn discard_staged_legacy_transcript_workflow_cutover(
        &self,
        source_generation: u64,
    ) -> LegacyTranscriptWorkflowCutoverProjection {
        match self.discard_transcript_workflow_cutover(source_generation) {
            Ok(changed) => {
                if changed {
                    self.notify_subscribers();
                }
                LegacyTranscriptWorkflowCutoverProjection::not_started()
            }
            Err(error) => LegacyTranscriptWorkflowCutoverProjection::blocked(error),
        }
    }
}

impl Pod0Facade {
    fn commit_transcript_workflow_cutover(
        &self,
        source_generation: u64,
    ) -> Result<pod0_storage::LegacyTranscriptWorkflowCutoverReport, StorageError> {
        let mut state = self.state();
        let store = state
            .store
            .clone()
            .ok_or(StorageError::TranscriptWorkflowConflict)?;
        let report = store
            .transcript_workflow_cutover_report()?
            .filter(|report| report_source_generation(report.state) == source_generation)
            .ok_or(StorageError::TranscriptWorkflowConflict)?;
        store.commit_legacy_transcript_workflow_cutover(
            source_generation,
            report.source_fingerprint,
            state.now().value,
        )?;
        state.advance_revision();
        state.rehydrate_transcript_workflows()?;
        store
            .transcript_workflow_cutover_report()?
            .ok_or(StorageError::TranscriptWorkflowConflict)
    }

    fn discard_transcript_workflow_cutover(
        &self,
        source_generation: u64,
    ) -> Result<bool, StorageError> {
        let mut state = self.state();
        let store = state
            .store
            .clone()
            .ok_or(StorageError::TranscriptWorkflowConflict)?;
        let changed = store.discard_legacy_transcript_workflow_cutover(source_generation)?;
        if changed {
            state.advance_revision();
        }
        Ok(changed)
    }

    fn cutover_result(
        &self,
        result: Result<pod0_storage::LegacyTranscriptWorkflowCutoverReport, StorageError>,
    ) -> LegacyTranscriptWorkflowCutoverProjection {
        match result {
            Ok(report) => {
                self.notify_subscribers();
                LegacyTranscriptWorkflowCutoverProjection::from_report(report)
            }
            Err(error) => LegacyTranscriptWorkflowCutoverProjection::blocked(error),
        }
    }
}

fn report_source_generation(state: TranscriptWorkflowAuthorityState) -> u64 {
    match state {
        TranscriptWorkflowAuthorityState::NotStarted => 0,
        TranscriptWorkflowAuthorityState::Staged { source_generation }
        | TranscriptWorkflowAuthorityState::Verified { source_generation }
        | TranscriptWorkflowAuthorityState::Authoritative { source_generation } => {
            source_generation
        }
    }
}

fn cutover_command_id(source_generation: u64) -> pod0_domain::CommandId {
    pod0_domain::CommandId::from_parts(0x504f_4430_5457_4355, source_generation)
}

fn cutover_cancellation_id(source_generation: u64) -> pod0_domain::CancellationId {
    pod0_domain::CancellationId::from_parts(0x504f_4430_5457_4341, source_generation)
}
