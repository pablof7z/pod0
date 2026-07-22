use pod0_application::OperationResult;

use crate::runtime_evidence_state::{EvidenceIndexCompletion, PendingEvidenceIndex};
use crate::runtime_state::FacadeState;

impl FacadeState {
    pub(super) fn finish_evidence_index(
        &mut self,
        pending: PendingEvidenceIndex,
        indexed_span_count: u32,
    ) {
        match pending.completion {
            EvidenceIndexCompletion::EvidenceRebuild => self.succeed(
                pending.command_id,
                Some(OperationResult::EvidenceRebuilt {
                    episode_id: pending.episode_id,
                    generation_id: pending.generation_id,
                    span_count: indexed_span_count,
                }),
            ),
            EvidenceIndexCompletion::RecallConfiguration {
                imported,
                revision,
                completed_episode_count,
                mut remaining,
            } => {
                let completed_episode_count = completed_episode_count.saturating_add(1);
                if let Some(next) = remaining.first().cloned() {
                    remaining.remove(0);
                    self.advance_evidence_index(PendingEvidenceIndex {
                        command_id: pending.command_id,
                        cancellation_id: pending.cancellation_id,
                        episode_id: next.episode_id,
                        generation_id: next.generation_id,
                        expected_span_count: next.expected_span_count,
                        requested_span_ids: Vec::new(),
                        completion: EvidenceIndexCompletion::RecallConfiguration {
                            imported,
                            revision,
                            completed_episode_count,
                            remaining,
                        },
                    });
                } else {
                    let result = imported.map_or(
                        OperationResult::RecallConfigurationUpdated {
                            revision,
                            reindexed_episode_count: completed_episode_count,
                        },
                        |imported| OperationResult::RecallConfigurationImported {
                            imported,
                            revision,
                        },
                    );
                    self.succeed(pending.command_id, Some(result));
                }
            }
        }
    }
}
