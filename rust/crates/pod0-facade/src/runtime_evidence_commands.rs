use pod0_application::{
    CoreFailureCode, EvidenceChunkPolicy, HostObservation, HostRequest, HostRequestEnvelope,
    OperationStage, RecallEmbeddingInput, TranscriptEvidenceInput, build_evidence_artifact,
};
use pod0_domain::{
    CommandId, EvidenceGenerationId, EvidenceSpanId, HostRequestId, TranscriptEvidenceArtifact,
    UnixTimestampMilliseconds,
};
use pod0_recall_index::{
    RECALL_INDEX_DIMENSIONS, RecallIndexError, RecallIndexPlan, RecallIndexSpan,
    RecallSpanEmbedding,
};
use sha2::{Digest, Sha256};

use crate::runtime_evidence_state::{EvidenceIndexCompletion, PendingEvidenceIndex};
use crate::runtime_state::{FacadeState, failure};

impl FacadeState {
    pub(super) fn rebuild_transcript_evidence(
        &mut self,
        envelope: &pod0_application::CommandEnvelope,
        input: TranscriptEvidenceInput,
        policy: EvidenceChunkPolicy,
    ) {
        self.start_evidence_index(
            envelope,
            input,
            policy,
            EvidenceIndexCompletion::EvidenceRebuild,
        );
    }

    pub(super) fn start_evidence_index(
        &mut self,
        envelope: &pod0_application::CommandEnvelope,
        input: TranscriptEvidenceInput,
        policy: EvidenceChunkPolicy,
        completion: EvidenceIndexCompletion,
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
        self.advance_evidence_index(PendingEvidenceIndex {
            command_id: envelope.command_id,
            cancellation_id: envelope.cancellation_id,
            episode_id,
            generation_id,
            expected_span_count: span_count,
            requested_span_ids: Vec::new(),
            completion,
        });
    }

    pub(super) fn finish_evidence_index_observation(
        &mut self,
        pending: PendingEvidenceIndex,
        observation: HostObservation,
    ) {
        match observation {
            HostObservation::RecallSpansEmbedded { embeddings, .. } => {
                let Some(artifact) = self.selected_artifact(&pending) else {
                    self.fail(pending.command_id, CoreFailureCode::StorageUnavailable);
                    return;
                };
                let spans = index_spans(&artifact)
                    .into_iter()
                    .filter(|span| pending.requested_span_ids.contains(&span.span_id))
                    .collect::<Vec<_>>();
                let observations = embeddings
                    .into_iter()
                    .map(|value| RecallSpanEmbedding {
                        span_id: value.span_id,
                        embedding: value.embedding,
                    })
                    .collect::<Vec<_>>();
                let interrupt = self.begin_recall_index_operation(pending.cancellation_id);
                let result = self.recall_index.cache_embeddings(
                    &spans,
                    &observations,
                    interrupt.cancellation(),
                );
                match result {
                    Ok(()) => {}
                    Err(RecallIndexError::Cancelled) => {
                        self.finish(
                            pending.command_id,
                            OperationStage::Cancelled,
                            Some(failure(CoreFailureCode::Cancelled)),
                            None,
                        );
                        return;
                    }
                    Err(_) => {
                        self.fail(pending.command_id, CoreFailureCode::HostRejected);
                        return;
                    }
                }
                self.advance_evidence_index(PendingEvidenceIndex {
                    requested_span_ids: Vec::new(),
                    ..pending
                });
            }
            HostObservation::Cancelled => self.finish(
                pending.command_id,
                OperationStage::Cancelled,
                Some(failure(CoreFailureCode::Cancelled)),
                None,
            ),
            HostObservation::Failed { .. } | HostObservation::Unsupported { .. } => {
                self.fail(pending.command_id, CoreFailureCode::HostUnavailable);
            }
            _ => self.fail(pending.command_id, CoreFailureCode::HostRejected),
        }
    }

    pub(super) fn advance_evidence_index(&mut self, mut pending: PendingEvidenceIndex) {
        let Some(artifact) = self.selected_artifact(&pending) else {
            self.fail(pending.command_id, CoreFailureCode::StorageUnavailable);
            return;
        };
        let spans = index_spans(&artifact);
        let interrupt = self.begin_recall_index_operation(pending.cancellation_id);
        let plan = self
            .recall_index
            .prepare_episode(&spans, interrupt.cancellation());
        match plan {
            Ok(RecallIndexPlan::Ready { indexed_span_count })
                if indexed_span_count == pending.expected_span_count =>
            {
                self.finish_evidence_index(pending, indexed_span_count);
            }
            Ok(RecallIndexPlan::NeedsEmbeddings { spans }) => {
                pending.requested_span_ids = spans.iter().map(|span| span.span_id).collect();
                let request_id = evidence_index_request_id(
                    pending.command_id,
                    pending.generation_id,
                    &pending.requested_span_ids,
                );
                let request = HostRequestEnvelope {
                    request_id,
                    command_id: pending.command_id,
                    cancellation_id: pending.cancellation_id,
                    issued_revision: self.revision,
                    deadline_at: Some(UnixTimestampMilliseconds::new(
                        self.now().value.saturating_add(600_000),
                    )),
                    request: HostRequest::EmbedRecallSpans {
                        episode_id: pending.episode_id,
                        generation_id: pending.generation_id,
                        provider: self.recall_configuration.embedding_provider,
                        model: self.recall_configuration.embedding_model.clone(),
                        spans: spans
                            .into_iter()
                            .map(|span| RecallEmbeddingInput {
                                span_id: span.span_id,
                                text: span.text,
                            })
                            .collect(),
                        maximum_dimensions: u16::try_from(RECALL_INDEX_DIMENSIONS)
                            .expect("bounded recall dimensions"),
                    },
                };
                if !self.host_requests.register(request.clone()) {
                    self.fail(pending.command_id, CoreFailureCode::InvalidCommand);
                    return;
                }
                self.pending_evidence_indexes
                    .insert(request_id, pending.clone());
                self.host_queue.push_back(request);
                self.finish(pending.command_id, OperationStage::Running, None, None);
            }
            Err(RecallIndexError::Cancelled) => self.finish(
                pending.command_id,
                OperationStage::Cancelled,
                Some(failure(CoreFailureCode::Cancelled)),
                None,
            ),
            Ok(RecallIndexPlan::Ready { .. }) | Err(_) => {
                self.fail(pending.command_id, CoreFailureCode::StorageUnavailable);
            }
        }
    }

    fn selected_artifact(
        &self,
        pending: &PendingEvidenceIndex,
    ) -> Option<TranscriptEvidenceArtifact> {
        let artifact = self
            .evidence_store
            .as_ref()?
            .selected_artifact(pending.episode_id)
            .ok()??;
        (artifact.generation_id == pending.generation_id
            && u32::try_from(artifact.spans.len()).ok() == Some(pending.expected_span_count))
        .then_some(artifact)
    }
}

fn index_spans(artifact: &TranscriptEvidenceArtifact) -> Vec<RecallIndexSpan> {
    artifact
        .spans
        .iter()
        .map(|span| RecallIndexSpan {
            span_id: span.span_id,
            generation_id: artifact.generation_id,
            episode_id: span.episode_id,
            podcast_id: span.podcast_id,
            text: span.text.clone(),
        })
        .collect()
}

pub(super) fn evidence_phase_command_id(
    generation_id: EvidenceGenerationId,
    phase: &[u8],
) -> CommandId {
    let mut hash = Sha256::new();
    hash.update(b"pod0-evidence-rebuild-phase-v1\0");
    hash.update(generation_id.into_bytes());
    hash.update(phase);
    id_from_digest(hash.finalize())
}

fn evidence_index_request_id(
    command_id: CommandId,
    generation_id: EvidenceGenerationId,
    spans: &[EvidenceSpanId],
) -> HostRequestId {
    let mut hash = Sha256::new();
    hash.update(b"pod0-evidence-embedding-request-v2\0");
    hash.update(command_id.into_bytes());
    hash.update(generation_id.into_bytes());
    for span_id in spans {
        hash.update(span_id.into_bytes());
    }
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
