use pod0_application::{
    AgentActionObservation, AgentActionOutcome, AgentToolAction, AgentTurnStage, AgentTurnState,
    AgentWorkflowAcceptance, ApplicationCommand, CommandEnvelope, RecallQuery, RecallStage,
};
use pod0_domain::{RecallQueryId, TranscriptSource, UnixTimestampMilliseconds};
use pod0_storage::{AgentAuditKind, AgentStore, StorageError};
use serde_json::{Value, json};

use crate::runtime_agent_modules::identity::{agent_command_id, continuation_model_fence_id};
use crate::runtime_agent_modules::persistence::persist_agent_update;
use crate::runtime_state::FacadeState;

impl FacadeState {
    pub(super) fn execute_agent_recall_action(
        &mut self,
        _agent_store: &AgentStore,
        state: AgentTurnState,
        observed_at: UnixTimestampMilliseconds,
    ) -> Result<(), StorageError> {
        let projection = state.projection();
        let proposal = projection
            .proposal
            .as_ref()
            .ok_or(StorageError::InvalidAgentState)?;
        let AgentToolAction::QueryTranscripts {
            query,
            scope,
            limit,
        } = &proposal.action
        else {
            return Err(StorageError::InvalidAgentState);
        };
        let query_id = RecallQueryId::from_bytes(proposal.proposal_id.into_bytes());
        if let Some(existing) = self.pending_agent_recalls.get(&query_id) {
            if *existing != projection.turn_id {
                return Err(StorageError::AgentTurnConflict);
            }
            let _ = self.finish_agent_recall_if_terminal(query_id, observed_at)?;
            return Ok(());
        }

        self.pending_agent_recalls
            .insert(query_id, projection.turn_id);
        let command_id = agent_command_id(b"agent-recall", projection.turn_id);
        let recall = RecallQuery {
            query_id,
            text: query.clone(),
            scope: *scope,
            limit: *limit,
        };
        self.start_recall(
            &CommandEnvelope {
                command_id,
                cancellation_id: state.cancellation_id(),
                expected_revision: None,
                command: ApplicationCommand::RecallQuery {
                    query: recall.clone(),
                },
            },
            recall,
        );
        if !self.recalls.contains_key(&query_id) {
            self.pending_agent_recalls.remove(&query_id);
            return Err(StorageError::InvalidAgentState);
        }
        let _ = self.finish_agent_recall_if_terminal(query_id, observed_at)?;
        Ok(())
    }

    pub(crate) fn finish_agent_recall_if_terminal(
        &mut self,
        query_id: RecallQueryId,
        observed_at: UnixTimestampMilliseconds,
    ) -> Result<bool, StorageError> {
        let Some(workflow) = self.recalls.get(&query_id) else {
            return Ok(false);
        };
        if !workflow.stage.is_terminal() {
            return Ok(false);
        }
        let Some(turn_id) = self.pending_agent_recalls.get(&query_id).copied() else {
            return Ok(false);
        };
        let store = self
            .agent_store
            .clone()
            .ok_or(StorageError::InvalidAgentState)?;
        let mut state = store
            .turn(turn_id)?
            .ok_or(StorageError::AgentTurnNotFound)?;
        if state.projection().stage != AgentTurnStage::Executing {
            self.retire_agent_recall(query_id);
            return Ok(true);
        }
        let before = state.projection();
        let proposal = before
            .proposal
            .as_ref()
            .ok_or(StorageError::InvalidAgentState)?;
        if !matches!(proposal.action, AgentToolAction::QueryTranscripts { .. }) {
            return Err(StorageError::InvalidAgentState);
        }
        let fence = before
            .execution_fence_id
            .ok_or(StorageError::InvalidAgentState)?;
        let result = self.agent_recall_result(query_id)?;
        let recall_evidence = self
            .recalls
            .get(&query_id)
            .ok_or(StorageError::InvalidAgentState)?
            .evidence
            .clone();
        if state.observe_action(AgentActionObservation {
            proposal_id: proposal.proposal_id,
            execution_fence_id: fence,
            outcome: AgentActionOutcome::Succeeded {
                bounded_result: result,
                artifact_id: None,
                recall_evidence,
            },
            observed_at,
        }) != AgentWorkflowAcceptance::Updated
        {
            return Err(StorageError::AgentTurnConflict);
        }
        let continuation_fence =
            continuation_model_fence_id(before.turn_id, state.projection().revision);
        if state.continue_after_commit(continuation_fence, observed_at)
            != AgentWorkflowAcceptance::Updated
        {
            return Err(StorageError::AgentTurnConflict);
        }
        let command_id = agent_command_id(b"agent-recall-result", before.turn_id);
        let state = persist_agent_update(
            &store,
            command_id,
            b"pod0:agent-recall-result:v1",
            AgentAuditKind::ActionObserved,
            before.revision,
            state,
            observed_at,
        )?;
        let _ = self.queue_agent_model_request(command_id, &state);
        self.retire_agent_recall(query_id);
        Ok(true)
    }

    pub(super) fn retire_agent_recall(&mut self, query_id: RecallQueryId) {
        self.pending_agent_recalls.remove(&query_id);
        self.recalls.remove(&query_id);
        self.pending_agent_recall_observations
            .retain(|_, pending| pending.query_id != query_id);
    }

    pub(super) fn retire_agent_recalls_for_turn(&mut self, turn_id: pod0_domain::AgentTurnId) {
        let query_ids = self
            .pending_agent_recalls
            .iter()
            .filter_map(|(query_id, candidate)| (*candidate == turn_id).then_some(*query_id))
            .collect::<Vec<_>>();
        for query_id in query_ids {
            self.retire_agent_recall(query_id);
        }
    }

    fn agent_recall_result(&self, query_id: RecallQueryId) -> Result<String, StorageError> {
        let workflow = self
            .recalls
            .get(&query_id)
            .ok_or(StorageError::InvalidAgentState)?;
        let evidence = workflow
            .evidence
            .iter()
            .map(|item| {
                let episode = self
                    .listening
                    .episodes
                    .iter()
                    .find(|episode| episode.episode_id == item.episode_id);
                let podcast = self
                    .listening
                    .podcasts
                    .iter()
                    .find(|podcast| podcast.podcast_id == item.podcast_id);
                json!({
                    "episode_id": opaque_id(item.episode_id.into_bytes()),
                    "podcast_id": opaque_id(item.podcast_id.into_bytes()),
                    "episode_title": episode.map(|value| value.title.as_str()),
                    "podcast_title": podcast.map(|value| value.title.as_str()),
                    "start_milliseconds": item.start_milliseconds,
                    "end_milliseconds": item.end_milliseconds,
                    "timestamp": timestamp(item.start_milliseconds),
                    "excerpt": item.excerpt,
                    "speaker_id": item.speaker_id.map(|id| opaque_id(id.into_bytes())),
                    "transcript_source": transcript_source(item.provenance.source),
                    "provider": item.provenance.provider,
                    "playable_reference": {
                        "episode_id": opaque_id(item.episode_id.into_bytes()),
                        "start_milliseconds": item.start_milliseconds,
                    },
                })
            })
            .collect::<Vec<Value>>();
        Ok(json!({
            "status": recall_stage(workflow.stage),
            "evidence": evidence,
        })
        .to_string())
    }
}

fn recall_stage(stage: RecallStage) -> &'static str {
    match stage {
        RecallStage::Queued => "queued",
        RecallStage::Running { .. } => "running",
        RecallStage::Ready => "ready",
        RecallStage::NoEvidence => "no_evidence",
        RecallStage::TranscriptMissing => "transcript_missing",
        RecallStage::IndexMissing => "index_missing",
        RecallStage::Indexing => "indexing",
        RecallStage::IndexUnavailable => "index_unavailable",
        RecallStage::ProviderUnavailable => "provider_unavailable",
        RecallStage::CorruptArtifact => "corrupt_artifact",
        RecallStage::Interrupted => "interrupted",
        RecallStage::Cancelled => "cancelled",
        RecallStage::Failed => "failed",
        RecallStage::Unsupported { .. } => "unsupported",
    }
}

fn transcript_source(source: TranscriptSource) -> &'static str {
    match source {
        TranscriptSource::Publisher => "publisher",
        TranscriptSource::Scribe => "scribe",
        TranscriptSource::Whisper => "whisper",
        TranscriptSource::OnDevice => "on_device",
        TranscriptSource::AssemblyAi => "assembly_ai",
        TranscriptSource::Other => "other",
        TranscriptSource::Unsupported { .. } => "unsupported",
    }
}

fn timestamp(milliseconds: u64) -> String {
    let seconds = milliseconds / 1_000;
    let hours = seconds / 3_600;
    let minutes = seconds % 3_600 / 60;
    let seconds = seconds % 60;
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

fn opaque_id(bytes: [u8; 16]) -> String {
    let hex = bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}
