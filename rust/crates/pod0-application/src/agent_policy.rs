use crate::agent_policy_shape::{
    episode_tool, no_argument_tool, podcast_tool, search_tool, text_tool,
};
use crate::{
    AgentAuthority, AgentExecutionKind, AgentToolAction, AgentToolClass, AgentToolName,
    AgentToolPolicy, MAX_AGENT_ACTION_TEXT_BYTES, MAX_AGENT_MODEL_REFERENCE_BYTES,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentActionValidationError {
    InvalidShape,
    EmptyValue,
    ValueTooLarge,
    InvalidRange,
    InvalidModelReference,
}

pub fn validate_agent_model_reference(value: &str) -> Result<(), AgentActionValidationError> {
    validate_text(value, MAX_AGENT_MODEL_REFERENCE_BYTES)
        .map_err(|_| AgentActionValidationError::InvalidModelReference)
}

pub fn validate_agent_action(action: &AgentToolAction) -> Result<(), AgentActionValidationError> {
    match action {
        AgentToolAction::NoArguments { tool } if no_argument_tool(*tool) => Ok(()),
        AgentToolAction::TextInput { tool, text } if text_tool(*tool) => {
            validate_text(text, MAX_AGENT_ACTION_TEXT_BYTES)
        }
        AgentToolAction::Search {
            tool,
            query,
            scope,
            limit,
        } if search_tool(*tool) => {
            validate_text(query, MAX_AGENT_ACTION_TEXT_BYTES)?;
            validate_optional_text(scope.as_deref(), 1_024)?;
            if (1..=25).contains(limit) {
                Ok(())
            } else {
                Err(AgentActionValidationError::InvalidRange)
            }
        }
        AgentToolAction::QueryTranscripts {
            query,
            scope,
            limit,
        } => {
            validate_text(query, crate::MAX_RECALL_QUERY_BYTES)?;
            if matches!(scope, crate::RecallScope::Unsupported { .. }) {
                return Err(AgentActionValidationError::InvalidShape);
            }
            if (1..=crate::MAX_AGENT_RECALL_EVIDENCE).contains(limit) {
                Ok(())
            } else {
                Err(AgentActionValidationError::InvalidRange)
            }
        }
        AgentToolAction::Episode { tool, .. } if episode_tool(*tool) => Ok(()),
        AgentToolAction::Podcast { tool, .. } if podcast_tool(*tool) => Ok(()),
        AgentToolAction::PlayEpisode {
            start_milliseconds,
            end_milliseconds,
            placement,
            ..
        } => {
            if matches!(placement, crate::QueuePlacement::Unsupported { .. }) {
                return Err(AgentActionValidationError::InvalidShape);
            }
            if matches!((start_milliseconds, end_milliseconds), (Some(start), Some(end)) if start >= end)
            {
                Err(AgentActionValidationError::InvalidRange)
            } else {
                Ok(())
            }
        }
        AgentToolAction::SetPlaybackRate { permille } => {
            if (500..=3_000).contains(permille) {
                Ok(())
            } else {
                Err(AgentActionValidationError::InvalidRange)
            }
        }
        AgentToolAction::SetSleepTimer {
            duration_milliseconds,
        } => {
            if duration_milliseconds.is_none_or(|value| (1_000..=86_400_000).contains(&value)) {
                Ok(())
            } else {
                Err(AgentActionValidationError::InvalidRange)
            }
        }
        AgentToolAction::CreateNote { text } | AgentToolAction::RecordMemory { text } => {
            validate_text(text, MAX_AGENT_ACTION_TEXT_BYTES)
        }
        AgentToolAction::Ask { question, context } => {
            validate_text(question, 8 * 1_024)?;
            validate_optional_text(context.as_deref(), 16 * 1_024)
        }
        AgentToolAction::ScheduleTask { task } => {
            validate_text(&task.label, crate::MAX_SCHEDULED_AGENT_LABEL_BYTES)?;
            validate_text(&task.prompt, crate::MAX_SCHEDULED_AGENT_PROMPT_BYTES)?;
            validate_agent_model_reference(&task.model_reference)?;
            if task.interval_milliseconds == 0 {
                Err(AgentActionValidationError::InvalidRange)
            } else {
                Ok(())
            }
        }
        AgentToolAction::CancelScheduledTask { .. } => Ok(()),
        AgentToolAction::ChangePodcastCategory { category, .. } => validate_text(category, 256),
        AgentToolAction::CreateClip {
            start_milliseconds,
            end_milliseconds,
            caption,
            frozen_transcript_text,
            ..
        } => {
            if start_milliseconds >= end_milliseconds {
                return Err(AgentActionValidationError::InvalidRange);
            }
            validate_optional_text(caption.as_deref(), 4 * 1_024)?;
            validate_text(frozen_transcript_text, MAX_AGENT_ACTION_TEXT_BYTES)
        }
        AgentToolAction::SubscribePodcast { feed_url }
        | AgentToolAction::IngestYoutubeVideo { url: feed_url } => {
            validate_text(feed_url, 8 * 1_024)
        }
        AgentToolAction::ConfigureAgentVoice { voice_id } => validate_text(voice_id, 256),
        AgentToolAction::CreatePodcast { title, description } => {
            validate_text(title, 1_024)?;
            validate_optional_text(Some(description), 16 * 1_024)
        }
        AgentToolAction::UpdatePodcast {
            title, description, ..
        } => {
            validate_text(title, 1_024)?;
            validate_optional_text(Some(description), 16 * 1_024)
        }
        AgentToolAction::GenerateTtsEpisode {
            title,
            script,
            voice_id,
            ..
        } => {
            validate_text(title, 1_024)?;
            validate_text(script, MAX_AGENT_ACTION_TEXT_BYTES)?;
            validate_optional_text(voice_id.as_deref(), 256)
        }
        AgentToolAction::GeneratePodcastArtwork { prompt, .. } => validate_text(prompt, 8 * 1_024),
        _ => Err(AgentActionValidationError::InvalidShape),
    }
}

#[must_use]
pub fn agent_tool_policy(tool: AgentToolName) -> AgentToolPolicy {
    use AgentAuthority::{DurableScopedGrant, DurableTurnGrant, None, OneShotApproval};
    use AgentToolClass::{
        DestructiveWrite, ExternalSideEffect, Publication, ReadOnly, ReversibleWrite,
        SecretBearing, SessionLocal,
    };
    use AgentToolName::*;
    let (classes, authority, execution) = match tool {
        UpgradeThinking => (
            vec![SessionLocal, ExternalSideEffect, SecretBearing],
            DurableScopedGrant,
            AgentExecutionKind::NativeCapability,
        ),
        UseSkill => (vec![SessionLocal], None, AgentExecutionKind::RustCommit),
        Ask => (
            vec![SessionLocal],
            None,
            AgentExecutionKind::NativeConversationPresentation,
        ),
        RecordMemory => (
            vec![ReversibleWrite, SecretBearing],
            OneShotApproval,
            AgentExecutionKind::RustCommit,
        ),
        ScheduleTask => (
            vec![ReversibleWrite, ExternalSideEffect],
            OneShotApproval,
            AgentExecutionKind::RustCommit,
        ),
        CancelScheduledTask | DeletePodcast | DeleteMyPodcast => (
            vec![DestructiveWrite],
            OneShotApproval,
            AgentExecutionKind::RustCommit,
        ),
        PerplexitySearch | SummarizeEpisode => (
            vec![ExternalSideEffect, SecretBearing],
            DurableScopedGrant,
            AgentExecutionKind::NativeCapability,
        ),
        ListAvailableVoices => (
            vec![ReadOnly, ExternalSideEffect, SecretBearing],
            DurableScopedGrant,
            AgentExecutionKind::NativeCapability,
        ),
        RequestTranscription | DownloadAndTranscribe => (
            vec![ReversibleWrite, ExternalSideEffect, SecretBearing],
            DurableScopedGrant,
            AgentExecutionKind::NativeCapability,
        ),
        GenerateTtsEpisode | IngestYoutubeVideo => (
            vec![ReversibleWrite, ExternalSideEffect, SecretBearing],
            OneShotApproval,
            AgentExecutionKind::NativeCapability,
        ),
        GeneratePodcastArtwork => (
            vec![ExternalSideEffect, SecretBearing, Publication],
            OneShotApproval,
            AgentExecutionKind::NativeCapabilityAndNmpPublication,
        ),
        PlayEpisode | PausePlayback | SetPlaybackRate | SetSleepTimer | DownloadEpisode
        | RefreshFeed | SubscribePodcast => (
            vec![ReversibleWrite, ExternalSideEffect],
            DurableTurnGrant,
            AgentExecutionKind::NativeCapability,
        ),
        SearchPodcastDirectory | SearchYoutube => (
            vec![ReadOnly, ExternalSideEffect],
            DurableTurnGrant,
            AgentExecutionKind::NativeCapability,
        ),
        ListScheduledTasks | ListConversations | SearchConversations | SearchEpisodes
        | QueryTranscripts | FindSimilarEpisodes | ListSubscriptions | ListPodcasts
        | ListCategories | ListEpisodes | ListInProgress | ListRecentUnplayed | ListMyPodcasts => (
            vec![ReadOnly, SecretBearing],
            DurableTurnGrant,
            AgentExecutionKind::RustProjection,
        ),
        CreateNote
        | MarkEpisodePlayed
        | MarkEpisodeUnplayed
        | ChangePodcastCategory
        | CreateClip
        | ConfigureAgentVoice
        | CreatePodcast
        | UpdatePodcast => (
            vec![ReversibleWrite],
            DurableTurnGrant,
            AgentExecutionKind::RustCommit,
        ),
    };
    AgentToolPolicy {
        tool,
        classes,
        authority,
        execution,
    }
}

fn validate_text(value: &str, maximum: usize) -> Result<(), AgentActionValidationError> {
    if value.trim().is_empty() {
        Err(AgentActionValidationError::EmptyValue)
    } else if value.len() > maximum {
        Err(AgentActionValidationError::ValueTooLarge)
    } else {
        Ok(())
    }
}

fn validate_optional_text(
    value: Option<&str>,
    maximum: usize,
) -> Result<(), AgentActionValidationError> {
    match value {
        Some(value) if !value.is_empty() => validate_text(value, maximum),
        _ => Ok(()),
    }
}
