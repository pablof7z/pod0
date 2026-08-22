use pod0_facade::{
    ApplicationCommand, AutoDownloadMode, AutoDownloadPolicy, PlaybackCommand,
    PlaybackRatePermille, Pod0Facade, Projection, ProjectionRequest, ProjectionScope,
    RecallConfigurationInput, TranscriptProvider, TranscriptStartPolicy,
    WorkflowConfigurationInput,
};

use crate::ids::{encode_id, parse_podcast_id};
use crate::mapping::library;
use crate::protocol::{
    AutoDownloadModeDto, AutoDownloadSettingsDto, CliError, PlaybackSettingsDto, RecallSettingsDto,
    RevisionedBoolDto, SettingMutation, SettingsDto, SubscriptionSettingsDto, TranscriptPolicyDto,
    WorkflowSettingsDto, WorkflowSettingsStateDto,
};

pub(crate) fn read(
    facade: &Pod0Facade,
    offset: u32,
    requested_limit: u16,
) -> Result<SettingsDto, CliError> {
    let limit = crate::protocol::bounded_page_limit(requested_limit);
    let (library, totals, _) = library(facade, offset, limit)?;
    let playback = match facade
        .snapshot(request(ProjectionScope::Playback, 0, 1))
        .projection
    {
        Projection::Playback { value } => value,
        _ => return Err(projection_error()),
    };
    let notifications = match facade
        .snapshot(request(
            ProjectionScope::NewEpisodeNotificationSettings,
            0,
            1,
        ))
        .projection
    {
        Projection::NewEpisodeNotificationSettings { value } => value,
        _ => return Err(projection_error()),
    };
    let recall = match facade
        .snapshot(request(ProjectionScope::RecallConfiguration, 0, 1))
        .projection
    {
        Projection::RecallConfiguration { value } => value,
        _ => return Err(projection_error()),
    };
    let workflow = facade
        .workflow_configuration()
        .map_err(|_| CliError::new("settings_read", "workflow settings are unavailable", true))?;
    let subscription_total = totals.1;

    Ok(SettingsDto {
        new_episode_notifications: RevisionedBoolDto {
            enabled: notifications.enabled,
            revision: notifications.revision.value,
        },
        subscription_offset: offset,
        subscription_limit: limit,
        subscription_total,
        subscriptions_has_more: usize::try_from(offset)
            .unwrap_or(usize::MAX)
            .saturating_add(library.subscriptions.len())
            < subscription_total,
        subscriptions: library
            .subscriptions
            .into_iter()
            .map(|subscription| {
                let (mode, latest_count) = match subscription.auto_download.mode {
                    AutoDownloadMode::Off => ("off", None),
                    AutoDownloadMode::Latest { count } => ("latest", Some(count)),
                    AutoDownloadMode::AllNew => ("all_new", None),
                    AutoDownloadMode::Unsupported { .. } => ("unsupported", None),
                };
                SubscriptionSettingsDto {
                    podcast_id: encode_id(subscription.podcast_id.into_bytes()),
                    notifications_enabled: subscription.notifications_enabled,
                    auto_download: AutoDownloadSettingsDto {
                        mode: mode.to_owned(),
                        latest_count,
                        wifi_only: subscription.auto_download.wifi_only,
                    },
                    transcript_start_policy: match subscription.transcript_start_policy {
                        TranscriptStartPolicy::Automatic => "automatic",
                        TranscriptStartPolicy::WhenPlayed => "when_played",
                        TranscriptStartPolicy::Unsupported { .. } => "unsupported",
                    }
                    .to_owned(),
                }
            })
            .collect(),
        playback: PlaybackSettingsDto {
            rate_permille: playback.rate.value,
            auto_mark_played_at_natural_end: playback.auto_mark_played_at_natural_end,
            auto_play_next: playback.auto_play_next,
            auto_skip_ads: playback.auto_skip_ads,
        },
        recall: RecallSettingsDto {
            revision: recall.revision.value,
            stored_embedding_model_id: recall.stored_embedding_model_id,
            reranker_enabled: recall.reranker_enabled,
            embedding_provider: format_recall_provider(recall.embedding_provider),
            embedding_model: recall.embedding_model,
        },
        workflow: workflow.map(|configuration| WorkflowSettingsStateDto {
            revision: configuration.revision.value,
            value: workflow_dto(configuration.value),
        }),
    })
}

pub(crate) fn command(
    facade: &Pod0Facade,
    setting: SettingMutation,
) -> Result<ApplicationCommand, CliError> {
    match setting {
        SettingMutation::NewEpisodeNotifications { enabled } => {
            Ok(ApplicationCommand::SetNewEpisodeNotificationsEnabled { enabled })
        }
        SettingMutation::SubscriptionNotifications {
            podcast_id,
            enabled,
        } => Ok(ApplicationCommand::SetSubscriptionNotifications {
            podcast_id: podcast(&podcast_id)?,
            enabled,
        }),
        SettingMutation::SubscriptionAutoDownload {
            podcast_id,
            mode,
            wifi_only,
        } => Ok(ApplicationCommand::SetSubscriptionAutoDownload {
            podcast_id: podcast(&podcast_id)?,
            policy: AutoDownloadPolicy {
                mode: match mode {
                    AutoDownloadModeDto::Off => AutoDownloadMode::Off,
                    AutoDownloadModeDto::Latest { count } => AutoDownloadMode::Latest { count },
                    AutoDownloadModeDto::AllNew => AutoDownloadMode::AllNew,
                },
                wifi_only,
            },
        }),
        SettingMutation::SubscriptionTranscriptPolicy { podcast_id, policy } => {
            Ok(ApplicationCommand::SetSubscriptionTranscriptStartPolicy {
                podcast_id: podcast(&podcast_id)?,
                policy: match policy {
                    TranscriptPolicyDto::Automatic => TranscriptStartPolicy::Automatic,
                    TranscriptPolicyDto::WhenPlayed => TranscriptStartPolicy::WhenPlayed,
                },
            })
        }
        SettingMutation::PlaybackRate { permille } => Ok(ApplicationCommand::Playback {
            command: PlaybackCommand::SetRate {
                rate: PlaybackRatePermille { value: permille },
            },
        }),
        SettingMutation::PlaybackPreferences {
            auto_mark_played_at_natural_end,
            auto_play_next,
            auto_skip_ads,
        } => Ok(ApplicationCommand::Playback {
            command: PlaybackCommand::SetPreferences {
                auto_mark_played_at_natural_end,
                auto_play_next,
                auto_skip_ads,
            },
        }),
        SettingMutation::Recall {
            stored_embedding_model_id,
            reranker_enabled,
        } => {
            let current = read(facade, 0, 1)?;
            Ok(ApplicationCommand::SetRecallConfiguration {
                expected_configuration_revision: pod0_facade::StateRevision::new(
                    current.recall.revision,
                ),
                configuration: RecallConfigurationInput {
                    stored_embedding_model_id,
                    reranker_enabled,
                },
            })
        }
        SettingMutation::Workflow { value } => {
            let current = facade.workflow_configuration().map_err(|_| {
                CliError::new("settings_read", "workflow settings are unavailable", true)
            })?;
            Ok(ApplicationCommand::SetWorkflowConfiguration {
                expected_configuration_revision: current
                    .map_or(pod0_facade::StateRevision::INITIAL, |value| value.revision),
                configuration: workflow_input(value)?,
            })
        }
    }
}

fn workflow_input(value: WorkflowSettingsDto) -> Result<WorkflowConfigurationInput, CliError> {
    let transcript_provider = match value.transcript_provider.as_str() {
        "assembly_ai" => TranscriptProvider::AssemblyAi,
        "eleven_labs_scribe" => TranscriptProvider::ElevenLabsScribe,
        "open_router_whisper" => TranscriptProvider::OpenRouterWhisper,
        "apple_speech" => TranscriptProvider::AppleSpeech,
        _ => {
            return Err(CliError::new(
                "invalid_setting",
                "unsupported transcript provider",
                false,
            ));
        }
    };
    Ok(WorkflowConfigurationInput {
        transcript_provider,
        eleven_labs_model: value.eleven_labs_model,
        assembly_ai_model: value.assembly_ai_model,
        open_router_model: value.open_router_model,
        auto_publisher_transcripts: value.auto_publisher_transcripts,
        auto_provider_transcripts: value.auto_provider_transcripts,
        chapter_model: value.chapter_model,
    })
}

fn workflow_dto(value: WorkflowConfigurationInput) -> WorkflowSettingsDto {
    WorkflowSettingsDto {
        transcript_provider: match value.transcript_provider {
            TranscriptProvider::AssemblyAi => "assembly_ai",
            TranscriptProvider::ElevenLabsScribe => "eleven_labs_scribe",
            TranscriptProvider::OpenRouterWhisper => "open_router_whisper",
            TranscriptProvider::AppleSpeech => "apple_speech",
            TranscriptProvider::Unsupported { .. } => "unsupported",
        }
        .to_owned(),
        eleven_labs_model: value.eleven_labs_model,
        assembly_ai_model: value.assembly_ai_model,
        open_router_model: value.open_router_model,
        auto_publisher_transcripts: value.auto_publisher_transcripts,
        auto_provider_transcripts: value.auto_provider_transcripts,
        chapter_model: value.chapter_model,
    }
}

fn format_recall_provider(value: pod0_facade::RecallEmbeddingProvider) -> String {
    match value {
        pod0_facade::RecallEmbeddingProvider::OpenRouter => "open_router",
        pod0_facade::RecallEmbeddingProvider::Ollama => "ollama",
        pod0_facade::RecallEmbeddingProvider::Unsupported { .. } => "unsupported",
    }
    .to_owned()
}

fn podcast(value: &str) -> Result<pod0_facade::PodcastId, CliError> {
    parse_podcast_id(value)
        .ok_or_else(|| CliError::new("invalid_id", "podcast_id must be 32 hex characters", false))
}

fn request(scope: ProjectionScope, offset: u32, max_items: u16) -> ProjectionRequest {
    ProjectionRequest {
        scope,
        offset,
        max_items,
    }
}

fn projection_error() -> CliError {
    CliError::new(
        "projection_unavailable",
        "the requested settings projection is unavailable",
        true,
    )
}
