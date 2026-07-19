use pod0_application::{
    CoreFailureCode, EvidenceChunkPolicy, HostObservation, HostRequest, HostRequestEnvelope,
    OperationResult, OperationStage, TranscriptEvidenceInput, build_evidence_artifact,
};
use pod0_domain::{CommandId, EvidenceGenerationId, HostRequestId, UnixTimestampMilliseconds};
use sha2::{Digest, Sha256};

use crate::runtime_evidence_state::PendingEvidenceIndex;
use crate::runtime_state::{FacadeState, failure};

impl FacadeState {
    pub(super) fn rebuild_transcript_evidence(
        &mut self,
        envelope: &pod0_application::CommandEnvelope,
        input: TranscriptEvidenceInput,
        policy: EvidenceChunkPolicy,
    ) {
        let Ok(artifact) = build_evidence_artifact(&input, policy) else {
            self.fail(envelope.command_id, CoreFailureCode::InvalidCommand);
            return;
        };
        let Some(store) = &self.evidence_store else {
            self.fail(envelope.command_id, CoreFailureCode::StorageUnavailable);
            return;
        };
        let now = self.now().value;
        let generation_id = artifact.generation_id;
        let episode_id = artifact.version.episode_id;
        let span_count = u32::try_from(artifact.spans.len()).unwrap_or(u32::MAX);
        let result = store
            .stage_artifact(
                evidence_phase_command_id(generation_id, b"stage"),
                &artifact,
                now,
            )
            .and_then(|_| {
                store.verify_generation(
                    evidence_phase_command_id(generation_id, b"verify"),
                    generation_id,
                    now,
                )
            })
            .and_then(|_| {
                store.select_generation(
                    evidence_phase_command_id(generation_id, b"select"),
                    episode_id,
                    generation_id,
                    now,
                )
            });
        if result.is_err() {
            self.fail(envelope.command_id, CoreFailureCode::StorageUnavailable);
            return;
        }

        let request_id = evidence_index_request_id(envelope.command_id, generation_id);
        let request = HostRequestEnvelope {
            request_id,
            command_id: envelope.command_id,
            cancellation_id: envelope.cancellation_id,
            issued_revision: self.revision,
            deadline_at: Some(UnixTimestampMilliseconds::new(now.saturating_add(600_000))),
            request: HostRequest::RebuildRecallIndex {
                episode_id,
                generation_id,
            },
        };
        if !self.host_requests.register(request.clone()) {
            self.fail(envelope.command_id, CoreFailureCode::InvalidCommand);
            return;
        }
        self.pending_evidence_indexes.insert(
            request_id,
            PendingEvidenceIndex {
                command_id: envelope.command_id,
                cancellation_id: envelope.cancellation_id,
                episode_id,
                generation_id,
                expected_span_count: span_count,
            },
        );
        self.host_queue.push_back(request);
        self.finish(envelope.command_id, OperationStage::Running, None, None);
    }

    pub(super) fn finish_evidence_index_observation(
        &mut self,
        pending: PendingEvidenceIndex,
        observation: HostObservation,
    ) {
        match observation {
            HostObservation::RecallIndexRebuilt {
                indexed_span_count, ..
            } if indexed_span_count == pending.expected_span_count
                && self.selected_generation_is(pending.episode_id, pending.generation_id) =>
            {
                self.succeed(
                    pending.command_id,
                    Some(OperationResult::EvidenceRebuilt {
                        episode_id: pending.episode_id,
                        generation_id: pending.generation_id,
                        span_count: indexed_span_count,
                    }),
                );
            }
            HostObservation::Cancelled => {
                self.finish(
                    pending.command_id,
                    OperationStage::Cancelled,
                    Some(failure(CoreFailureCode::Cancelled)),
                    None,
                );
            }
            HostObservation::Failed { .. } | HostObservation::Unsupported { .. } => {
                self.fail(pending.command_id, CoreFailureCode::HostUnavailable);
            }
            _ => self.fail(pending.command_id, CoreFailureCode::HostRejected),
        }
    }

    fn selected_generation_is(
        &self,
        episode_id: pod0_domain::EpisodeId,
        generation_id: EvidenceGenerationId,
    ) -> bool {
        self.evidence_store
            .as_ref()
            .and_then(|store| store.selected_generation(episode_id).ok().flatten())
            .is_some_and(|selected| selected.generation_id == generation_id)
    }
}

fn evidence_phase_command_id(generation_id: EvidenceGenerationId, phase: &[u8]) -> CommandId {
    let mut hash = Sha256::new();
    hash.update(b"pod0-evidence-rebuild-phase-v1\0");
    hash.update(generation_id.into_bytes());
    hash.update(phase);
    id_from_digest(hash.finalize())
}

fn evidence_index_request_id(
    command_id: CommandId,
    generation_id: EvidenceGenerationId,
) -> HostRequestId {
    let mut hash = Sha256::new();
    hash.update(b"pod0-evidence-index-request-v1\0");
    hash.update(command_id.into_bytes());
    hash.update(generation_id.into_bytes());
    let digest = hash.finalize();
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    HostRequestId::from_bytes(bytes)
}

fn id_from_digest(digest: impl AsRef<[u8]>) -> CommandId {
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest.as_ref()[..16]);
    CommandId::from_bytes(bytes)
}
