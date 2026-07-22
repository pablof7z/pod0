use pod0_application::{
    CommandEnvelope, CoreFailureCode, HostRequest, HostRequestEnvelope, MAX_RECALL_EVIDENCE,
    MAX_RECALL_QUERY_BYTES, OperationResult, OperationStage, RecallEvidenceProjection, RecallQuery,
    RecallScope, RecallStage,
};
use pod0_domain::{
    CancellationId, CommandId, EpisodeId, HostRequestId, RecallQueryId, TranscriptArtifactStatus,
    UnixTimestampMilliseconds,
};
use pod0_recall_index::RECALL_INDEX_DIMENSIONS;
use sha2::{Digest, Sha256};

use crate::runtime_recall_state::{PendingRecall, RecallHostPhase, RecallWorkflow};
use crate::runtime_state::{FacadeState, failure};

impl FacadeState {
    pub(super) fn start_recall(&mut self, envelope: &CommandEnvelope, query: RecallQuery) {
        if self.recalls.contains_key(&query.query_id) {
            self.fail(envelope.command_id, CoreFailureCode::InvalidCommand);
            return;
        }
        let normalized_text = query.text.split_whitespace().collect::<Vec<_>>().join(" ");
        let query_id = query.query_id;
        let workflow = RecallWorkflow::new(
            envelope.command_id,
            envelope.cancellation_id,
            query_id,
            query.scope,
            normalized_text,
            query.limit,
        );
        self.recalls.insert(query_id, workflow);

        if self.recalls[&query_id].normalized_text.is_empty()
            || self.recalls[&query_id].normalized_text.len() > MAX_RECALL_QUERY_BYTES
            || query.limit == 0
            || usize::from(query.limit) > MAX_RECALL_EVIDENCE
        {
            self.fail_recall(
                query_id,
                RecallStage::Failed,
                CoreFailureCode::InvalidCommand,
            );
            return;
        }
        if let RecallScope::Unsupported { wire_code } = query.scope {
            self.fail_recall(
                query_id,
                RecallStage::Unsupported { wire_code },
                CoreFailureCode::Unsupported { wire_code },
            );
            return;
        }
        if self.scope_has_pending_evidence_index(query.scope) {
            self.complete_recall(query_id, RecallStage::Indexing, Vec::new());
            return;
        }
        let has_evidence = self
            .evidence_store
            .as_ref()
            .ok_or(pod0_storage::StorageError::EvidenceNotFound)
            .and_then(|store| match query.scope {
                RecallScope::Library => store.has_any_selected_evidence(),
                RecallScope::Podcast { podcast_id } => {
                    store.has_selected_evidence_for_podcast(podcast_id)
                }
                RecallScope::Episode { episode_id } => {
                    store.has_selected_evidence_for_episode(episode_id)
                }
                RecallScope::Unsupported { .. } => Ok(false),
            });
        match has_evidence {
            Ok(false) => {
                let stage = if self.scope_has_available_transcript(query.scope) {
                    RecallStage::IndexMissing
                } else {
                    RecallStage::TranscriptMissing
                };
                self.complete_recall(query_id, stage, Vec::new());
                return;
            }
            Err(_) => {
                self.fail_recall(
                    query_id,
                    RecallStage::IndexUnavailable,
                    CoreFailureCode::StorageUnavailable,
                );
                return;
            }
            Ok(true) => {}
        }
        match self.recall_index.has_ready_scope(query.scope) {
            Ok(true) => {}
            Ok(false) => {
                self.complete_recall(query_id, RecallStage::IndexMissing, Vec::new());
                return;
            }
            Err(_) => {
                self.fail_recall(
                    query_id,
                    RecallStage::IndexUnavailable,
                    CoreFailureCode::StorageUnavailable,
                );
                return;
            }
        }
        self.queue_recall_request(
            query_id,
            RecallHostPhase::Embedding,
            HostRequest::EmbedRecallQuery {
                query_id,
                provider: self.recall_configuration.embedding_provider,
                model: self.recall_configuration.embedding_model.clone(),
                text: self.recalls[&query_id].normalized_text.clone(),
                maximum_dimensions: u16::try_from(RECALL_INDEX_DIMENSIONS)
                    .expect("bounded recall dimensions"),
            },
        );
    }

    fn scope_has_pending_evidence_index(&self, scope: RecallScope) -> bool {
        self.pending_evidence_indexes
            .values()
            .any(|pending| self.episode_matches_scope(pending.episode_id, scope))
    }

    fn scope_has_available_transcript(&self, scope: RecallScope) -> bool {
        self.listening.episodes.iter().any(|episode| {
            self.episode_matches_scope(episode.episode_id, scope)
                && matches!(
                    episode.transcript,
                    TranscriptArtifactStatus::Available { .. }
                )
        })
    }

    fn episode_matches_scope(&self, episode_id: EpisodeId, scope: RecallScope) -> bool {
        let episode = self
            .listening
            .episodes
            .iter()
            .find(|episode| episode.episode_id == episode_id);
        match scope {
            RecallScope::Library => episode.is_some(),
            RecallScope::Podcast { podcast_id } => {
                episode.is_some_and(|episode| episode.podcast_id == podcast_id)
            }
            RecallScope::Episode {
                episode_id: scoped_id,
            } => episode_id == scoped_id,
            RecallScope::Unsupported { .. } => false,
        }
    }

    pub(super) fn queue_recall_request(
        &mut self,
        query_id: RecallQueryId,
        phase: RecallHostPhase,
        request: HostRequest,
    ) {
        let Some(workflow) = self.recalls.get(&query_id) else {
            return;
        };
        let command_id = workflow.command_id;
        let cancellation_id = workflow.cancellation_id;
        let request_id = recall_request_id(command_id, phase);
        let envelope = HostRequestEnvelope {
            request_id,
            command_id,
            cancellation_id,
            issued_revision: self.revision,
            deadline_at: Some(UnixTimestampMilliseconds::new(
                self.now().value.saturating_add(30_000),
            )),
            request,
        };
        if self.host_requests.register(envelope.clone()) {
            self.pending_recalls.insert(
                request_id,
                PendingRecall {
                    query_id,
                    cancellation_id,
                    phase,
                },
            );
            self.host_queue.push_back(envelope);
            self.finish(command_id, OperationStage::Running, None, None);
        } else {
            self.fail_recall(
                query_id,
                RecallStage::Failed,
                CoreFailureCode::InvalidCommand,
            );
        }
    }

    pub(super) fn complete_recall(
        &mut self,
        query_id: RecallQueryId,
        stage: RecallStage,
        evidence: Vec<RecallEvidenceProjection>,
    ) {
        let Some(workflow) = self.recalls.get_mut(&query_id) else {
            return;
        };
        workflow.stage = stage;
        workflow.failure = None;
        workflow.evidence = evidence;
        let command_id = workflow.command_id;
        let evidence_count = u16::try_from(workflow.evidence.len()).unwrap_or(u16::MAX);
        self.succeed(
            command_id,
            Some(OperationResult::RecallFinished {
                query_id,
                evidence_count,
            }),
        );
    }

    pub(super) fn fail_recall(
        &mut self,
        query_id: RecallQueryId,
        stage: RecallStage,
        code: CoreFailureCode,
    ) {
        let Some(workflow) = self.recalls.get_mut(&query_id) else {
            return;
        };
        let recall_failure = failure(code);
        workflow.stage = stage;
        workflow.failure = Some(recall_failure.clone());
        workflow.evidence.clear();
        let command_id = workflow.command_id;
        self.finish(
            command_id,
            OperationStage::Failed,
            Some(recall_failure),
            None,
        );
    }

    pub(super) fn cancel_recall(&mut self, cancellation_id: CancellationId) {
        let query_ids = self
            .recalls
            .values()
            .filter(|workflow| {
                workflow.cancellation_id == cancellation_id && !workflow.stage.is_terminal()
            })
            .map(|workflow| workflow.query_id)
            .collect::<Vec<_>>();
        self.pending_recalls
            .retain(|_, pending| pending.cancellation_id != cancellation_id);
        for query_id in query_ids {
            let Some(workflow) = self.recalls.get_mut(&query_id) else {
                continue;
            };
            let recall_failure = failure(CoreFailureCode::Cancelled);
            workflow.stage = RecallStage::Cancelled;
            workflow.failure = Some(recall_failure.clone());
            workflow.evidence.clear();
            let command_id = workflow.command_id;
            self.finish(
                command_id,
                OperationStage::Cancelled,
                Some(recall_failure),
                None,
            );
        }
    }
}

fn recall_request_id(command_id: CommandId, phase: RecallHostPhase) -> HostRequestId {
    let tag = match phase {
        RecallHostPhase::Embedding => b"embedding".as_slice(),
        RecallHostPhase::Reranking => b"reranking".as_slice(),
    };
    let mut hash = Sha256::new();
    hash.update(b"pod0-recall-host-request-v1\0");
    hash.update(command_id.into_bytes());
    hash.update(tag);
    let digest = hash.finalize();
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    HostRequestId::from_bytes(bytes)
}
