use std::collections::BTreeMap;

use pod0_domain::{
    EpisodeId, ListeningDomainSnapshot, TranscriptArtifactStatus, TranscriptStartPolicy,
};

use crate::{
    TranscriptWorkflowConfiguration, TranscriptWorkflowOrigin, WorkflowCapabilitySnapshot,
    WorkflowConfiguration,
};

pub const MAX_WORKFLOW_RECONCILE_EPISODES_PER_PAGE: usize = 32;

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum WorkflowReconcileIntent {
    EnsurePublisherChapters {
        episode_id: EpisodeId,
    },
    EnsureTranscript {
        episode_id: EpisodeId,
        origin: TranscriptWorkflowOrigin,
        configuration: TranscriptWorkflowConfiguration,
    },
    EnsureModelChapters {
        episode_id: EpisodeId,
        configured_model: String,
    },
    ReconcileScheduledRuns,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct WorkflowReconcilePlan {
    pub intents: Vec<WorkflowReconcileIntent>,
    pub next_episode_offset: Option<u32>,
}

#[must_use]
pub fn plan_workflow_reconciliation(
    listening: &ListeningDomainSnapshot,
    configuration: &WorkflowConfiguration,
    capabilities: &WorkflowCapabilitySnapshot,
) -> WorkflowReconcilePlan {
    plan_workflow_reconciliation_page(listening, configuration, capabilities, 0)
}

#[must_use]
pub fn plan_workflow_reconciliation_page(
    listening: &ListeningDomainSnapshot,
    configuration: &WorkflowConfiguration,
    capabilities: &WorkflowCapabilitySnapshot,
    episode_offset: u32,
) -> WorkflowReconcilePlan {
    if configuration.schema_version != crate::WORKFLOW_CONFIGURATION_SCHEMA_VERSION
        || configuration.value.validate().is_err()
        || capabilities.local_audio.len() > crate::MAX_WORKFLOW_LOCAL_AUDIO_CAPABILITIES
    {
        return WorkflowReconcilePlan {
            intents: Vec::new(),
            next_episode_offset: None,
        };
    }
    let policies = listening
        .subscriptions
        .iter()
        .map(|value| (value.podcast_id, value.transcript_start_policy))
        .collect::<BTreeMap<_, _>>();
    let local_audio = capabilities
        .local_audio
        .iter()
        .map(|value| (value.episode_id, value.local_audio_url.as_str()))
        .collect::<BTreeMap<_, _>>();
    let selected_model = configuration.value.selected_transcript_model();
    let credential_available = capabilities
        .credentials
        .available(configuration.value.transcript_provider);
    let mut intents = Vec::new();
    let offset = usize::try_from(episode_offset).unwrap_or(usize::MAX);
    for episode in listening
        .episodes
        .iter()
        .skip(offset)
        .take(MAX_WORKFLOW_RECONCILE_EPISODES_PER_PAGE)
    {
        if episode.feed_metadata.chapters_url.is_some() {
            intents.push(WorkflowReconcileIntent::EnsurePublisherChapters {
                episode_id: episode.episode_id,
            });
        }
        let automatic =
            policies.get(&episode.podcast_id).copied() == Some(TranscriptStartPolicy::Automatic);
        let publisher_available = episode.feed_metadata.publisher_transcript.is_some();
        let transcript_requested = automatic
            && ((configuration.value.auto_publisher_transcripts && publisher_available)
                || configuration.value.auto_provider_transcripts);
        if transcript_requested && let Some(model) = selected_model {
            intents.push(WorkflowReconcileIntent::EnsureTranscript {
                episode_id: episode.episode_id,
                origin: TranscriptWorkflowOrigin::Automatic,
                configuration: TranscriptWorkflowConfiguration {
                    provider: configuration.value.transcript_provider,
                    model: model.to_owned(),
                    local_audio_url: local_audio
                        .get(&episode.episode_id)
                        .map(ToString::to_string),
                    credential_available,
                    auto_publisher_enabled: configuration.value.auto_publisher_transcripts,
                    auto_provider_enabled: configuration.value.auto_provider_transcripts,
                },
            });
        }
        if matches!(
            episode.transcript,
            TranscriptArtifactStatus::Available { .. }
        ) {
            intents.push(WorkflowReconcileIntent::EnsureModelChapters {
                episode_id: episode.episode_id,
                configured_model: configuration.value.chapter_model.clone(),
            });
        }
    }
    let consumed = listening
        .episodes
        .len()
        .saturating_sub(offset)
        .min(MAX_WORKFLOW_RECONCILE_EPISODES_PER_PAGE);
    let next_episode_offset = offset
        .checked_add(consumed)
        .filter(|value| *value < listening.episodes.len())
        .and_then(|value| u32::try_from(value).ok());
    if next_episode_offset.is_none() {
        intents.push(WorkflowReconcileIntent::ReconcileScheduledRuns);
    }
    WorkflowReconcilePlan {
        intents,
        next_episode_offset,
    }
}
