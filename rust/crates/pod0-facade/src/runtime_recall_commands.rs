use pod0_application::{
    CommandEnvelope, CoreFailureCode, MAX_RECALL_EVIDENCE, MAX_RECALL_QUERY_BYTES, OperationStage,
    RecallQuery, RecallScope, RecallStage,
};
use pod0_domain::{EpisodeId, TranscriptArtifactStatus};

use crate::runtime_recall_state::RecallWorkflow;
use crate::runtime_state::{FacadeState, failure};

impl FacadeState {
    pub(super) fn rehydrate_recall_queries(&mut self) -> Result<(), pod0_storage::StorageError> {
        let Some(store) = self.store.clone() else {
            return Ok(());
        };
        for workflow in store.recall_query_workflows()? {
            self.revision =
                pod0_domain::StateRevision::new(self.revision.value.max(workflow.revision.value));
            self.recalls.insert(
                workflow.query.query_id,
                RecallWorkflow {
                    command_id: workflow.command_id,
                    cancellation_id: workflow.cancellation_id,
                    query_id: workflow.query.query_id,
                    scope: workflow.query.scope,
                    normalized_text: workflow.query.text,
                    limit: workflow.query.limit,
                    stage: workflow.stage,
                    failure: workflow.failure,
                    evidence: workflow.evidence,
                },
            );
        }
        Ok(())
    }

    pub(super) fn start_recall(&mut self, envelope: &CommandEnvelope, query: RecallQuery) {
        let Some(store) = self.store.clone() else {
            self.fail(envelope.command_id, CoreFailureCode::StorageUnavailable);
            return;
        };
        let normalized = query.text.split_whitespace().collect::<Vec<_>>().join(" ");
        let mut normalized_query = query.clone();
        normalized_query.text = normalized;
        let (initial_stage, initial_failure) = self.initial_recall_state(&normalized_query);
        match store.start_recall_query(
            envelope.command_id,
            &crate::runtime_command_fingerprint::command_fingerprint(&envelope.command),
            envelope.cancellation_id,
            normalized_query,
            initial_stage,
            initial_failure.clone(),
            self.now(),
        ) {
            Ok(workflow) => {
                self.recalls.insert(
                    query.query_id,
                    RecallWorkflow {
                        command_id: workflow.command_id,
                        cancellation_id: workflow.cancellation_id,
                        query_id: workflow.query.query_id,
                        scope: workflow.query.scope,
                        normalized_text: workflow.query.text,
                        limit: workflow.query.limit,
                        stage: workflow.stage,
                        failure: workflow.failure.clone(),
                        evidence: workflow.evidence,
                    },
                );
                if workflow.stage.is_terminal() {
                    if let Some(failure) = workflow.failure.clone() {
                        self.finish(
                            envelope.command_id,
                            OperationStage::Failed,
                            Some(failure),
                            None,
                        );
                    } else {
                        self.succeed(
                            envelope.command_id,
                            Some(pod0_application::OperationResult::RecallFinished {
                                query_id: workflow.query.query_id,
                                evidence_count: 0,
                            }),
                        );
                    }
                } else {
                    self.finish(envelope.command_id, OperationStage::Running, None, None);
                }
            }
            Err(_) => self.fail(envelope.command_id, CoreFailureCode::InvalidCommand),
        }
    }

    fn initial_recall_state(
        &self,
        query: &RecallQuery,
    ) -> (RecallStage, Option<pod0_application::CoreFailure>) {
        let failed = |stage, code| (stage, Some(failure(code)));
        if query.text.is_empty()
            || query.text.len() > MAX_RECALL_QUERY_BYTES
            || query.limit == 0
            || usize::from(query.limit) > MAX_RECALL_EVIDENCE
        {
            return failed(RecallStage::Failed, CoreFailureCode::InvalidCommand);
        }
        if let RecallScope::Unsupported { wire_code } = query.scope {
            return failed(
                RecallStage::Unsupported { wire_code },
                CoreFailureCode::Unsupported { wire_code },
            );
        }
        if self.scope_has_pending_evidence_index(query.scope) {
            return (RecallStage::Indexing, None);
        }
        let has_evidence =
            self.evidence_store
                .as_ref()
                .ok_or(())
                .and_then(|store| match query.scope {
                    RecallScope::Library => store.has_any_selected_evidence().map_err(|_| ()),
                    RecallScope::Podcast { podcast_id } => store
                        .has_selected_evidence_for_podcast(podcast_id)
                        .map_err(|_| ()),
                    RecallScope::Episode { episode_id } => store
                        .has_selected_evidence_for_episode(episode_id)
                        .map_err(|_| ()),
                    RecallScope::Unsupported { .. } => Ok(false),
                });
        match has_evidence {
            Ok(false) => {
                return (
                    if self.scope_has_available_transcript(query.scope) {
                        RecallStage::IndexMissing
                    } else {
                        RecallStage::TranscriptMissing
                    },
                    None,
                );
            }
            Err(()) => {
                return failed(
                    RecallStage::IndexUnavailable,
                    CoreFailureCode::StorageUnavailable,
                );
            }
            Ok(true) => {}
        }
        match self.recall_index.has_ready_scope(query.scope) {
            Ok(true) => (
                RecallStage::Running {
                    phase: pod0_application::RecallPhase::Retrieving,
                },
                None,
            ),
            Ok(false) => (RecallStage::IndexMissing, None),
            Err(_) => failed(
                RecallStage::IndexUnavailable,
                CoreFailureCode::StorageUnavailable,
            ),
        }
    }

    fn scope_has_pending_evidence_index(&self, scope: RecallScope) -> bool {
        self.store
            .as_ref()
            .and_then(|store| store.active_evidence_embedding_effects().ok())
            .unwrap_or_default()
            .iter()
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
}
