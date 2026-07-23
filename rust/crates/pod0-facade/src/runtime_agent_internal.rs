use pod0_application::{
    AgentActionObservation, AgentActionOutcome, AgentToolAction, AgentToolName, AgentTurnState,
    AgentWorkflowAcceptance,
};
use pod0_domain::{ClipId, ClipSource, CommandId, CompletionStatus, NoteAuthor, NoteKind};
use pod0_storage::{AgentAuditKind, AgentStore, StorageError};
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::runtime_agent_modules::identity::agent_command_id;
use crate::runtime_agent_modules::persistence::persist_agent_update;
use crate::runtime_state::FacadeState;

impl FacadeState {
    pub(super) fn execute_internal_agent_action(
        &mut self,
        agent_store: &AgentStore,
        mut state: AgentTurnState,
        observed_at: pod0_domain::UnixTimestampMilliseconds,
    ) -> Result<(), StorageError> {
        let before = state.projection();
        let proposal = before
            .proposal
            .clone()
            .ok_or(StorageError::InvalidAgentState)?;
        let fence = before
            .execution_fence_id
            .ok_or(StorageError::InvalidAgentState)?;
        let outcome = match self.perform_internal_agent_action(
            proposal.proposal_id,
            &proposal.action,
            observed_at,
        ) {
            Ok(result) => AgentActionOutcome::Succeeded {
                bounded_result: result,
                artifact_id: None,
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
        let command_id = agent_command_id(b"internal-action-result", before.turn_id);
        let _ = persist_agent_update(
            agent_store,
            command_id,
            b"pod0:agent-internal-action-result:v1",
            AgentAuditKind::ActionObserved,
            before.revision,
            state,
            observed_at,
        )?;
        Ok(())
    }

    fn perform_internal_agent_action(
        &mut self,
        proposal_id: pod0_domain::AgentProposalId,
        action: &AgentToolAction,
        observed_at: pod0_domain::UnixTimestampMilliseconds,
    ) -> Result<String, &'static str> {
        match action {
            AgentToolAction::TextInput {
                tool: AgentToolName::UseSkill,
                text,
            } => Ok(json!({ "enabled_skill": text }).to_string()),
            AgentToolAction::CreateNote { text } => {
                let store = self.store.as_ref().ok_or("agent_store_unavailable")?;
                let command_id = CommandId::from_bytes(proposal_id.into_bytes());
                let fingerprint = commit_fingerprint("create-note", proposal_id);
                let (_, note_id) = store
                    .create_note(
                        command_id,
                        &fingerprint,
                        text,
                        NoteKind::Free,
                        NoteAuthor::Agent,
                        None,
                        observed_at.value,
                    )
                    .map_err(|_| "agent_note_commit_failed")?;
                self.reload_notes()
                    .map_err(|_| "agent_note_reload_failed")?;
                Ok(json!({
                    "note_id": opaque_id_string(note_id.into_bytes()),
                    "saved": true
                })
                .to_string())
            }
            AgentToolAction::CreateClip {
                episode_id,
                podcast_id,
                start_milliseconds,
                end_milliseconds,
                caption,
                frozen_transcript_text,
            } => {
                let store = self.store.as_ref().ok_or("agent_store_unavailable")?;
                let command_id = CommandId::from_bytes(proposal_id.into_bytes());
                let clip_id = ClipId::from_bytes(proposal_id.into_bytes());
                let fingerprint = commit_fingerprint("create-clip", proposal_id);
                store
                    .create_clip(
                        command_id,
                        &fingerprint,
                        clip_id,
                        *episode_id,
                        *podcast_id,
                        *start_milliseconds,
                        *end_milliseconds,
                        caption.as_deref(),
                        None,
                        frozen_transcript_text,
                        ClipSource::Agent,
                        observed_at.value,
                    )
                    .map_err(|_| "agent_clip_commit_failed")?;
                self.reload_clips()
                    .map_err(|_| "agent_clip_reload_failed")?;
                Ok(json!({
                    "clip_id": opaque_id_string(clip_id.into_bytes()),
                    "saved": true
                })
                .to_string())
            }
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
}

fn episode_json(episode: &pod0_domain::EpisodeRecord) -> serde_json::Value {
    json!({
        "episode_id": opaque_id_string(episode.episode_id.into_bytes()),
        "podcast_id": opaque_id_string(episode.podcast_id.into_bytes()),
        "title": episode.title,
        "position_milliseconds": episode.listening.resume_position_milliseconds,
        "completed": matches!(episode.listening.completion, CompletionStatus::Completed { .. })
    })
}

fn commit_fingerprint(domain: &str, proposal_id: pod0_domain::AgentProposalId) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"pod0:agent-internal-commit:v1\0");
    hasher.update(domain.as_bytes());
    hasher.update([0]);
    hasher.update(proposal_id.into_bytes());
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn opaque_id_string(bytes: [u8; 16]) -> String {
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
