use std::collections::BTreeSet;

use pod0_application::{
    CommandEnvelope, CoreFailureCode, HostFailureCode, HostObservationReceipt,
    HostObservationRejection, LeasedHostObservationEnvelope, OperationResult, OperationStage,
    RecallIndexCutoverHostOutcome,
};

use crate::runtime_chapter_model_receipts::{persisted, retain};
use crate::runtime_leased_observations::rejected_payload;
use crate::runtime_state::{FacadeState, failure};

impl FacadeState {
    pub(super) fn start_recall_index_cutover(&mut self, envelope: &CommandEnvelope) {
        let Some(store) = self.store.clone() else {
            self.fail(envelope.command_id, CoreFailureCode::StorageUnavailable);
            return;
        };
        let already_committed = match self.recall_index.legacy_cutover_is_committed() {
            Ok(value) => value,
            Err(_) => {
                self.fail(envelope.command_id, CoreFailureCode::StorageUnavailable);
                return;
            }
        };
        let prerequisites_ready = self
            .selected_active_generations_are_ready()
            .unwrap_or(false);
        let outcome = store.start_recall_index_cutover(
            envelope.command_id,
            &crate::runtime_command_fingerprint::command_fingerprint(&envelope.command),
            envelope.cancellation_id,
            already_committed,
            prerequisites_ready,
            self.now(),
        );
        match outcome {
            Ok(pod0_storage::RecallIndexCutoverStartOutcome::Authorized(_)) => {
                self.finish(envelope.command_id, OperationStage::Running, None, None);
            }
            Ok(pod0_storage::RecallIndexCutoverStartOutcome::AlreadyComplete) => {
                self.succeed(envelope.command_id, Some(cutover_result(0)));
            }
            Ok(pod0_storage::RecallIndexCutoverStartOutcome::MissingPrerequisite) => {
                self.fail(envelope.command_id, CoreFailureCode::StorageUnavailable);
            }
            Err(_) => self.fail(envelope.command_id, CoreFailureCode::InvalidCommand),
        }
    }

    pub(super) fn record_leased_recall_index_cutover_observation(
        &mut self,
        leased: LeasedHostObservationEnvelope,
        observation: pod0_application::DurableRecallIndexCutoverHostObservation,
    ) -> (bool, HostObservationReceipt) {
        let request_id = observation.request_id;
        let Some(store) = self.store.clone() else {
            return (false, retain(request_id));
        };
        let outcome = observation.outcome.clone();
        let (workflow, replayed) = match store.commit_recall_index_cutover_observation(
            leased.lease,
            observation,
            self.now(),
        ) {
            Ok(value) => value,
            Err(pod0_storage::StorageError::RevisionConflict) => return (false, stale(request_id)),
            Err(_) => return (false, retain(request_id)),
        };
        if replayed {
            return (false, duplicate(request_id));
        }
        self.host_requests.retire(request_id);
        match outcome {
            RecallIndexCutoverHostOutcome::ArtifactsRemoved { removed_file_count } => {
                if self
                    .finish_durable_recall_cutover(workflow.command_id, removed_file_count)
                    .is_err()
                {
                    self.fail(workflow.command_id, CoreFailureCode::StorageUnavailable);
                    return (true, persisted(request_id, false));
                }
            }
            RecallIndexCutoverHostOutcome::Cancelled => self.finish(
                workflow.command_id,
                OperationStage::Cancelled,
                Some(failure(CoreFailureCode::Cancelled)),
                None,
            ),
            RecallIndexCutoverHostOutcome::Failed { code, .. } => {
                self.fail(workflow.command_id, cutover_host_failure(code));
            }
        }
        self.advance_revision();
        (true, persisted(request_id, true))
    }

    pub(super) fn recover_recall_index_cutover(
        &mut self,
    ) -> Result<(), pod0_storage::StorageError> {
        let Some(store) = self.store.clone() else {
            return Ok(());
        };
        let Some(workflow) = store.recall_index_cutover_workflow()? else {
            return Ok(());
        };
        if let pod0_storage::RecallIndexCutoverStage::HostObserved { removed_file_count } =
            workflow.stage
        {
            self.finish_durable_recall_cutover(workflow.command_id, removed_file_count)?;
        }
        Ok(())
    }

    fn finish_durable_recall_cutover(
        &mut self,
        command_id: pod0_domain::CommandId,
        removed_file_count: u32,
    ) -> Result<(), pod0_storage::StorageError> {
        let count = u8::try_from(removed_file_count)
            .map_err(|_| pod0_storage::StorageError::InvalidActivity)?;
        let receipt = self
            .recall_index
            .commit_legacy_cutover(count)
            .map_err(|_| pod0_storage::StorageError::InvalidActivity)?;
        let store = self
            .store
            .clone()
            .ok_or(pod0_storage::StorageError::CutoverNotAuthoritative)?;
        store.finalize_recall_index_cutover(command_id, removed_file_count, self.now())?;
        self.succeed(
            command_id,
            Some(OperationResult::RecallIndexCutoverCommitted {
                schema_version: receipt.schema_version,
                removed_legacy_file_count: receipt.removed_legacy_file_count,
            }),
        );
        Ok(())
    }

    fn selected_active_generations_are_ready(&self) -> Result<bool, ()> {
        let selected = self
            .evidence_store
            .as_ref()
            .map(|store| store.selected_generations())
            .transpose()
            .map_err(|_| ())?
            .unwrap_or_default();
        let active = self
            .listening
            .episodes
            .iter()
            .map(|episode| episode.episode_id)
            .collect::<BTreeSet<_>>();
        for generation in selected {
            if active.contains(&generation.episode_id)
                && !self
                    .recall_index
                    .generation_is_ready(
                        generation.episode_id,
                        generation.generation_id,
                        generation.span_count,
                    )
                    .map_err(|_| ())?
            {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

fn cutover_result(removed_legacy_file_count: u8) -> OperationResult {
    OperationResult::RecallIndexCutoverCommitted {
        schema_version: pod0_recall_index::RECALL_INDEX_SCHEMA_VERSION,
        removed_legacy_file_count,
    }
}
fn cutover_host_failure(code: HostFailureCode) -> CoreFailureCode {
    match code {
        HostFailureCode::Unauthorized => CoreFailureCode::Unauthorized,
        HostFailureCode::PermissionDenied => CoreFailureCode::HostRejected,
        HostFailureCode::Unsupported { wire_code } => CoreFailureCode::Unsupported { wire_code },
        _ => CoreFailureCode::HostUnavailable,
    }
}
fn stale(id: pod0_domain::HostRequestId) -> HostObservationReceipt {
    rejected_payload(id, HostObservationRejection::StaleWorkflow)
}
fn duplicate(id: pod0_domain::HostRequestId) -> HostObservationReceipt {
    rejected_payload(id, HostObservationRejection::Duplicate)
}
