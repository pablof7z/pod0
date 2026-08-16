use pod0_application::{
    CoreFailureCode, EvidenceChunkPolicy, HostObservation, OperationStage, RecallEmbeddingInput,
    TranscriptEvidenceInput, build_evidence_artifact,
};
use pod0_domain::{TranscriptEvidenceArtifact, UnixTimestampMilliseconds};
use pod0_recall_index::{RecallIndexError, RecallIndexPlan, RecallSpanEmbedding};

use crate::runtime_command_fingerprint::command_fingerprint_digest;
use crate::runtime_evidence_effect_helpers::{
    evidence_effect_command_id, evidence_effect_fingerprint, evidence_index_request_id,
};
use crate::runtime_evidence_state::{EvidenceIndexCompletion, PendingEvidenceIndex};
use crate::runtime_state::{FacadeState, failure};

pub(super) use crate::runtime_evidence_effect_helpers::{index_spans, pending_from_effect};

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
        let Some(library) = self.store.clone() else {
            self.fail(envelope.command_id, CoreFailureCode::StorageUnavailable);
            return;
        };
        if self.evidence_store.is_none() {
            self.fail(envelope.command_id, CoreFailureCode::StorageUnavailable);
            return;
        }
        let now = self.now().value;
        let generation_id = artifact.generation_id;
        let episode_id = artifact.version.episode_id;
        let span_count = u32::try_from(artifact.spans.len()).unwrap_or(u32::MAX);
        let pending = PendingEvidenceIndex {
            command_id: envelope.command_id,
            cancellation_id: envelope.cancellation_id,
            episode_id,
            generation_id,
            expected_span_count: span_count,
            requested_span_ids: Vec::new(),
            completion,
        };
        let effect =
            match self.prepare_evidence_effect_for_artifact(pending.clone(), artifact.clone()) {
                Ok(value) => value,
                Err(code) => {
                    self.fail(envelope.command_id, code);
                    return;
                }
            };
        let result = library.commit_evidence_rebuild(
            envelope.command_id,
            command_fingerprint_digest(&envelope.command),
            &artifact,
            effect.clone(),
            now,
        );
        if result.is_err() {
            self.fail(envelope.command_id, CoreFailureCode::StorageUnavailable);
            return;
        }
        if effect.is_some() {
            self.finish(envelope.command_id, OperationStage::Running, None, None);
        } else {
            self.finish_evidence_index(pending, span_count);
        }
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

    pub(super) fn advance_evidence_index(&mut self, pending: PendingEvidenceIndex) {
        let store = self.store.clone();
        match self.prepare_evidence_effect(pending.clone()) {
            Ok(Some(effect)) => {
                let Some(artifact) = self.selected_artifact(&pending) else {
                    self.fail(pending.command_id, CoreFailureCode::StorageUnavailable);
                    return;
                };
                let ingress = evidence_effect_command_id(effect.request_id);
                let result = store
                    .as_ref()
                    .ok_or(pod0_storage::StorageError::CutoverNotAuthoritative)
                    .and_then(|store| {
                        store.commit_evidence_rebuild(
                            ingress,
                            evidence_effect_fingerprint(&effect),
                            &artifact,
                            Some(effect),
                            self.now().value,
                        )
                    });
                if result.is_ok() {
                    self.finish(pending.command_id, OperationStage::Running, None, None);
                } else {
                    self.fail(pending.command_id, CoreFailureCode::StorageUnavailable);
                }
            }
            Ok(None) => self.finish_evidence_index(pending.clone(), pending.expected_span_count),
            Err(code) => self.fail(pending.command_id, code),
        }
    }

    pub(super) fn prepare_evidence_effect(
        &mut self,
        pending: PendingEvidenceIndex,
    ) -> Result<Option<pod0_application::DurableEvidenceEmbeddingEffectRequest>, CoreFailureCode>
    {
        let Some(artifact) = self.selected_artifact(&pending) else {
            return Err(CoreFailureCode::StorageUnavailable);
        };
        self.prepare_evidence_effect_for_artifact(pending, artifact)
    }

    pub(super) fn prepare_evidence_effect_for_artifact(
        &mut self,
        pending: PendingEvidenceIndex,
        artifact: TranscriptEvidenceArtifact,
    ) -> Result<Option<pod0_application::DurableEvidenceEmbeddingEffectRequest>, CoreFailureCode>
    {
        let spans = index_spans(&artifact);
        let interrupt = self.begin_recall_index_operation(pending.cancellation_id);
        let plan = self
            .recall_index
            .prepare_episode(&spans, interrupt.cancellation());
        match plan {
            Ok(RecallIndexPlan::Ready { indexed_span_count })
                if indexed_span_count == pending.expected_span_count =>
            {
                Ok(None)
            }
            Ok(RecallIndexPlan::NeedsEmbeddings { spans }) => {
                let requested_span_ids = spans.iter().map(|span| span.span_id).collect::<Vec<_>>();
                let request_id = evidence_index_request_id(
                    pending.command_id,
                    pending.generation_id,
                    &requested_span_ids,
                );
                Ok(Some(
                    pod0_application::DurableEvidenceEmbeddingEffectRequest {
                        request_id,
                        command_id: pending.command_id,
                        cancellation_id: pending.cancellation_id,
                        issued_revision: self.revision,
                        deadline_at: UnixTimestampMilliseconds::new(
                            self.now().value.saturating_add(600_000),
                        ),
                        episode_id: pending.episode_id,
                        generation_id: pending.generation_id,
                        expected_span_count: pending.expected_span_count,
                        provider: self.recall_configuration.embedding_provider,
                        model: self.recall_configuration.embedding_model.clone(),
                        spans: spans
                            .into_iter()
                            .map(|span| RecallEmbeddingInput {
                                span_id: span.span_id,
                                text: span.text,
                            })
                            .collect(),
                        completion: pending.completion.durable(),
                    },
                ))
            }
            Err(RecallIndexError::Cancelled) => Err(CoreFailureCode::Cancelled),
            Ok(RecallIndexPlan::Ready { .. }) | Err(_) => Err(CoreFailureCode::StorageUnavailable),
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
