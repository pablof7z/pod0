use std::collections::BTreeSet;

use pod0_application::{
    CommandEnvelope, CoreFailureCode, HostObservation, HostRequest, HostRequestEnvelope,
    OperationResult, OperationStage,
};
use pod0_domain::{CancellationId, CommandId, HostRequestId, UnixTimestampMilliseconds};

use crate::runtime_state::{FacadeState, failure};

#[derive(Clone, Copy, Debug)]
pub(super) struct PendingRecallCutover {
    pub command_id: CommandId,
    pub cancellation_id: CancellationId,
}

impl FacadeState {
    pub(super) fn start_recall_index_cutover(&mut self, envelope: &CommandEnvelope) {
        match self.recall_index.legacy_cutover_is_committed() {
            Ok(true) => {
                self.succeed(
                    envelope.command_id,
                    Some(OperationResult::RecallIndexCutoverCommitted {
                        schema_version: pod0_recall_index::RECALL_INDEX_SCHEMA_VERSION,
                        removed_legacy_file_count: 0,
                    }),
                );
                return;
            }
            Ok(false) => {}
            Err(_) => {
                self.fail(envelope.command_id, CoreFailureCode::StorageUnavailable);
                return;
            }
        }
        match self.selected_active_generations_are_ready() {
            Ok(true) => {}
            Ok(false) | Err(()) => {
                self.fail(envelope.command_id, CoreFailureCode::StorageUnavailable);
                return;
            }
        }

        let request_id = HostRequestId::from_bytes(envelope.command_id.into_bytes());
        let request = HostRequestEnvelope {
            request_id,
            command_id: envelope.command_id,
            cancellation_id: envelope.cancellation_id,
            issued_revision: self.revision,
            deadline_at: Some(UnixTimestampMilliseconds::new(
                self.now().value.saturating_add(60_000),
            )),
            request: HostRequest::RemoveLegacyRecallIndexArtifacts,
        };
        if !self.host_requests.register(request.clone()) {
            self.fail(envelope.command_id, CoreFailureCode::InvalidCommand);
            return;
        }
        self.pending_recall_cutovers.insert(
            request_id,
            PendingRecallCutover {
                command_id: envelope.command_id,
                cancellation_id: envelope.cancellation_id,
            },
        );
        self.host_queue.push_back(request);
        self.finish(envelope.command_id, OperationStage::Running, None, None);
    }

    pub(super) fn finish_recall_index_cutover(
        &mut self,
        pending: PendingRecallCutover,
        observation: HostObservation,
    ) {
        match observation {
            HostObservation::LegacyRecallIndexArtifactsRemoved { removed_file_count } => {
                match self.recall_index.commit_legacy_cutover(removed_file_count) {
                    Ok(receipt) => self.succeed(
                        pending.command_id,
                        Some(OperationResult::RecallIndexCutoverCommitted {
                            schema_version: receipt.schema_version,
                            removed_legacy_file_count: receipt.removed_legacy_file_count,
                        }),
                    ),
                    Err(_) => {
                        self.fail(pending.command_id, CoreFailureCode::StorageUnavailable);
                    }
                }
            }
            HostObservation::Cancelled => self.finish(
                pending.command_id,
                OperationStage::Cancelled,
                Some(failure(CoreFailureCode::Cancelled)),
                None,
            ),
            HostObservation::Failed { .. } => {
                self.fail(pending.command_id, CoreFailureCode::HostUnavailable);
            }
            HostObservation::Unsupported { wire_code } => self.fail(
                pending.command_id,
                CoreFailureCode::Unsupported { wire_code },
            ),
            _ => self.fail(pending.command_id, CoreFailureCode::HostRejected),
        }
    }

    fn selected_active_generations_are_ready(&self) -> Result<bool, ()> {
        let selected = self
            .evidence_store
            .as_ref()
            .map(|store| store.selected_generations())
            .transpose()
            .map_err(|_| ())?
            .unwrap_or_default();
        let active_episode_ids = self
            .listening
            .episodes
            .iter()
            .map(|episode| episode.episode_id)
            .collect::<BTreeSet<_>>();
        for generation in selected {
            if !active_episode_ids.contains(&generation.episode_id) {
                continue;
            }
            if !self
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
