use std::collections::BTreeSet;

use pod0_application::{
    CommandEnvelope, CoreFailureCode, HostCancellationRequest, OperationResult,
};
use pod0_domain::{ContentDigest, RecallConfigurationInput, StateRevision};

use crate::runtime_evidence_state::{
    EvidenceIndexCompletion, EvidenceIndexTarget, PendingEvidenceIndex,
};
use crate::runtime_state::FacadeState;

impl FacadeState {
    pub(super) fn import_legacy_recall_configuration(
        &mut self,
        envelope: &CommandEnvelope,
        fingerprint: &str,
        configuration: RecallConfigurationInput,
        source_generation: ContentDigest,
    ) {
        let result = self.store.as_ref().map_or(
            Err(pod0_storage::StorageError::CutoverNotAuthoritative),
            |store| {
                store.import_legacy_recall_configuration(
                    envelope.command_id,
                    fingerprint,
                    configuration,
                    source_generation,
                    self.now().value,
                )
            },
        );
        self.apply_recall_configuration_mutation(envelope, result, Some);
    }

    pub(super) fn set_recall_configuration(
        &mut self,
        envelope: &CommandEnvelope,
        fingerprint: &str,
        expected_revision: StateRevision,
        configuration: RecallConfigurationInput,
    ) {
        let result = self.store.as_ref().map_or(
            Err(pod0_storage::StorageError::CutoverNotAuthoritative),
            |store| {
                store.set_recall_configuration(
                    envelope.command_id,
                    fingerprint,
                    expected_revision,
                    configuration,
                    self.now().value,
                )
            },
        );
        self.apply_recall_configuration_mutation(envelope, result, |_| None);
    }

    fn apply_recall_configuration_mutation(
        &mut self,
        envelope: &CommandEnvelope,
        result: Result<pod0_storage::RecallConfigurationMutation, pod0_storage::StorageError>,
        imported: impl FnOnce(bool) -> Option<bool>,
    ) {
        let mutation = match result {
            Ok(value) => value,
            Err(pod0_storage::StorageError::InvalidRecallConfiguration) => {
                self.fail(envelope.command_id, CoreFailureCode::InvalidCommand);
                return;
            }
            Err(pod0_storage::StorageError::RevisionConflict) => {
                self.fail(envelope.command_id, CoreFailureCode::RevisionConflict);
                return;
            }
            Err(_) => {
                self.fail(envelope.command_id, CoreFailureCode::StorageUnavailable);
                return;
            }
        };
        self.revision = StateRevision::new(
            self.revision
                .value
                .max(mutation.configuration.revision.value),
        );
        self.recall_configuration = mutation.configuration.clone();
        let embedding_changed = match self
            .recall_index
            .activate_embedding_space(mutation.configuration.embedding_space_id)
        {
            Ok(value) => value,
            Err(_) => {
                self.fail(envelope.command_id, CoreFailureCode::StorageUnavailable);
                return;
            }
        };
        let imported = imported(mutation.imported);
        if !embedding_changed {
            let result = imported.map_or(
                OperationResult::RecallConfigurationUpdated {
                    revision: mutation.configuration.revision,
                    reindexed_episode_count: 0,
                },
                |imported| OperationResult::RecallConfigurationImported {
                    imported,
                    revision: mutation.configuration.revision,
                },
            );
            self.succeed(envelope.command_id, Some(result));
            return;
        }
        self.cancel_stale_recall_capabilities();
        let targets = match self.recall_reindex_targets() {
            Ok(value) => value,
            Err(_) => {
                self.fail(envelope.command_id, CoreFailureCode::StorageUnavailable);
                return;
            }
        };
        let Some(first) = targets.first().cloned() else {
            let result = imported.map_or(
                OperationResult::RecallConfigurationUpdated {
                    revision: mutation.configuration.revision,
                    reindexed_episode_count: 0,
                },
                |imported| OperationResult::RecallConfigurationImported {
                    imported,
                    revision: mutation.configuration.revision,
                },
            );
            self.succeed(envelope.command_id, Some(result));
            return;
        };
        self.advance_evidence_index(PendingEvidenceIndex {
            command_id: envelope.command_id,
            cancellation_id: envelope.cancellation_id,
            episode_id: first.episode_id,
            generation_id: first.generation_id,
            expected_span_count: first.expected_span_count,
            requested_span_ids: Vec::new(),
            completion: EvidenceIndexCompletion::RecallConfiguration {
                imported,
                revision: mutation.configuration.revision,
                completed_episode_count: 0,
                remaining: targets.into_iter().skip(1).collect(),
            },
        });
    }

    fn recall_reindex_targets(
        &self,
    ) -> Result<Vec<EvidenceIndexTarget>, pod0_storage::StorageError> {
        self.evidence_store
            .as_ref()
            .ok_or(pod0_storage::StorageError::EvidenceNotFound)?
            .selected_generations()
            .map(|values| {
                values
                    .into_iter()
                    .filter(|value| value.span_count > 0)
                    .map(|value| EvidenceIndexTarget {
                        episode_id: value.episode_id,
                        generation_id: value.generation_id,
                        expected_span_count: value.span_count,
                    })
                    .collect()
            })
    }

    fn cancel_stale_recall_capabilities(&mut self) {
        let requests = self
            .pending_evidence_indexes
            .iter()
            .map(|(request_id, pending)| (*request_id, pending.cancellation_id))
            .chain(
                self.pending_recalls
                    .iter()
                    .map(|(request_id, pending)| (*request_id, pending.cancellation_id)),
            )
            .collect::<Vec<_>>();
        for (request_id, cancellation_id) in &requests {
            self.host_cancellations.push_back(HostCancellationRequest {
                request_id: *request_id,
                cancellation_id: *cancellation_id,
            });
        }
        let cancellation_ids = requests
            .into_iter()
            .map(|(_, cancellation_id)| cancellation_id)
            .collect::<BTreeSet<_>>();
        for cancellation_id in cancellation_ids {
            self.cancel_operation(cancellation_id);
        }
    }
}
