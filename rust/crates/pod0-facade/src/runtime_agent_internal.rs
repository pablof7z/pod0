use pod0_application::{
    AgentActionObservation, AgentActionOutcome, AgentToolAction, AgentToolName, AgentTurnState,
    AgentWorkflowAcceptance,
};
use pod0_domain::CompletionStatus;
use pod0_storage::{AgentAuditKind, AgentStore, StorageError};
use serde_json::json;

use crate::runtime_agent_modules::identity::{agent_command_id, continuation_model_fence_id};
use crate::runtime_agent_modules::persistence::persist_agent_update;
use crate::runtime_state::FacadeState;

impl FacadeState {
    pub(super) fn execute_internal_agent_action(
        &mut self,
        agent_store: &AgentStore,
        state: AgentTurnState,
        observed_at: pod0_domain::UnixTimestampMilliseconds,
    ) -> Result<(), StorageError> {
        if matches!(
            state
                .projection()
                .proposal
                .as_ref()
                .map(|value| &value.action),
            Some(AgentToolAction::QueryTranscripts { .. })
        ) {
            return self.execute_agent_recall_action(agent_store, state, observed_at);
        }
        let mut state = state;
        let before = state.projection();
        let proposal = before
            .proposal
            .clone()
            .ok_or(StorageError::InvalidAgentState)?;
        let fence = before
            .execution_fence_id
            .ok_or(StorageError::InvalidAgentState)?;
        let outcome = match self.perform_internal_agent_action(&proposal.action) {
            Ok(result) => AgentActionOutcome::Succeeded {
                bounded_result: result,
                artifact_id: None,
                recall_evidence: Vec::new(),
            },
            Err(error) => AgentActionOutcome::Failed {
                safe_detail: Some(error.to_owned()),
            },
        };
        if state.observe_action(AgentActionObservation {
            proposal_id: proposal.proposal_id,
            execution_fence_id: fence,
            outcome,
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
        let command_id = agent_command_id(b"internal-action-result", before.turn_id);
        let state = persist_agent_update(
            agent_store,
            command_id,
            b"pod0:agent-internal-action-result:v1",
            AgentAuditKind::ActionObserved,
            before.revision,
            state,
            observed_at,
        )?;
        let _ = self.queue_agent_model_request(command_id, &state);
        Ok(())
    }

    pub(super) fn perform_internal_agent_action(
        &mut self,
        action: &AgentToolAction,
    ) -> Result<String, &'static str> {
        match action {
            AgentToolAction::TextInput {
                tool: AgentToolName::UseSkill,
                text,
            } => Ok(json!({ "enabled_skill": text }).to_string()),
            AgentToolAction::NoArguments { tool }
                if matches!(
                    tool,
                    AgentToolName::ListSubscriptions
                        | AgentToolName::ListPodcasts
                        | AgentToolName::ListInProgress
                        | AgentToolName::ListRecentUnplayed
                ) =>
            {
                self.list_library_action(*tool)
            }
            AgentToolAction::NoArguments {
                tool: AgentToolName::ListCategories,
            } => self.list_categories_action(),
            AgentToolAction::Podcast {
                tool: AgentToolName::ListEpisodes,
                podcast_id,
            } => {
                let rows = self
                    .listening
                    .episodes
                    .iter()
                    .filter(|episode| episode.podcast_id == *podcast_id)
                    .take(25)
                    .map(episode_json)
                    .collect::<Vec<_>>();
                Ok(json!({ "episodes": rows }).to_string())
            }
            AgentToolAction::Search {
                tool: AgentToolName::SearchEpisodes,
                query,
                limit,
                ..
            } => {
                let query = query.to_lowercase();
                let rows = self
                    .listening
                    .episodes
                    .iter()
                    .filter(|episode| {
                        episode.title.to_lowercase().contains(&query)
                            || episode.description.to_lowercase().contains(&query)
                    })
                    .take(usize::from(*limit))
                    .map(episode_json)
                    .collect::<Vec<_>>();
                Ok(json!({ "episodes": rows }).to_string())
            }
            _ => Err("agent_internal_executor_unavailable"),
        }
    }

    fn list_library_action(&self, tool: AgentToolName) -> Result<String, &'static str> {
        match tool {
            AgentToolName::ListSubscriptions | AgentToolName::ListPodcasts => {
                let subscribed_only = tool == AgentToolName::ListSubscriptions;
                let rows = self
                    .listening
                    .podcasts
                    .iter()
                    .filter(|podcast| {
                        !subscribed_only
                            || self
                                .listening
                                .subscriptions
                                .iter()
                                .any(|value| value.podcast_id == podcast.podcast_id)
                    })
                    .take(25)
                    .map(|podcast| {
                        json!({
                            "podcast_id": opaque_id_string(podcast.podcast_id.into_bytes()),
                            "title": podcast.title,
                            "author": podcast.author
                        })
                    })
                    .collect::<Vec<_>>();
                Ok(json!({ "podcasts": rows }).to_string())
            }
            AgentToolName::ListInProgress | AgentToolName::ListRecentUnplayed => {
                let in_progress = tool == AgentToolName::ListInProgress;
                let rows = self
                    .listening
                    .episodes
                    .iter()
                    .filter(|episode| {
                        if in_progress {
                            episode.listening.resume_position_milliseconds > 0
                                && !matches!(
                                    episode.listening.completion,
                                    CompletionStatus::Completed { .. }
                                )
                        } else {
                            !matches!(
                                episode.listening.completion,
                                CompletionStatus::Completed { .. }
                            )
                        }
                    })
                    .take(25)
                    .map(episode_json)
                    .collect::<Vec<_>>();
                Ok(json!({ "episodes": rows }).to_string())
            }
            _ => Err("agent_internal_executor_unavailable"),
        }
    }

    fn list_categories_action(&self) -> Result<String, &'static str> {
        let store = self.store.as_ref().ok_or("library_unavailable")?;
        let snapshot = store
            .category_snapshot()
            .map_err(|_| "category_projection_unavailable")?;
        let rows = snapshot
            .categories
            .into_iter()
            .take(25)
            .map(|category| {
                json!({
                    "category_id": opaque_id_string(category.category_id.into_bytes()),
                    "name": category.name,
                    "description": category.description,
                    "color_hex": category.color_hex,
                    "member_count": category.members.len(),
                })
            })
            .collect::<Vec<_>>();
        Ok(json!({ "categories": rows }).to_string())
    }
}

include!("runtime_agent_internal_helpers.rs");
