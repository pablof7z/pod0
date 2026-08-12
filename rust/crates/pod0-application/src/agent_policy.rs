use crate::{
    AgentAuthority, AgentExecutionKind, AgentToolClass, AgentToolName, AgentToolPolicy,
    MAX_AGENT_MODEL_REFERENCE_BYTES,
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

pub use crate::agent_action_validation::validate_agent_action;

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
        // `write_category` is one primitive covering create, edit, and
        // delete, so its class depends on the call rather than the tool. It
        // is classed on its worst case: deleting discards curation the user
        // may have spent real attention on, and nothing else records it.
        WriteCategory => (
            vec![DestructiveWrite],
            OneShotApproval,
            AgentExecutionKind::RustCommit,
        ),
        TagItems => (
            vec![ReversibleWrite],
            DurableTurnGrant,
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
        SearchPodcastDirectory => (
            vec![ReversibleWrite, ExternalSideEffect],
            DurableTurnGrant,
            AgentExecutionKind::NativeCapability,
        ),
        SearchYoutube => (
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

pub(crate) fn validate_text(value: &str, maximum: usize) -> Result<(), AgentActionValidationError> {
    if value.trim().is_empty() {
        Err(AgentActionValidationError::EmptyValue)
    } else if value.len() > maximum {
        Err(AgentActionValidationError::ValueTooLarge)
    } else {
        Ok(())
    }
}

/// Accepts `#RRGGBB` / `#RRGGBBAA` only. The tint crosses into a SwiftUI
/// surface, so the closed form is enforced here rather than trusted from the
/// model and parsed defensively downstream.
pub(crate) fn validate_optional_color_hex(
    value: Option<&str>,
) -> Result<(), AgentActionValidationError> {
    let Some(value) = value else { return Ok(()) };
    let Some(digits) = value.strip_prefix('#') else {
        return Err(AgentActionValidationError::InvalidShape);
    };
    if !matches!(digits.len(), 6 | 8) || !digits.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(AgentActionValidationError::InvalidShape);
    }
    Ok(())
}

pub(crate) fn validate_optional_text(
    value: Option<&str>,
    maximum: usize,
) -> Result<(), AgentActionValidationError> {
    match value {
        Some(value) if !value.is_empty() => validate_text(value, maximum),
        _ => Ok(()),
    }
}
