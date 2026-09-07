use std::sync::Arc;

use pod0_facade::{
    AgentMessageRole, ApplicationCommand, CommandEnvelope, OperationResult, PlaybackCommand,
    Projection, ProjectionRequest, ProjectionScope,
};

use super::Shell;
use crate::ids::{encode_id, parse_conversation_id, parse_episode_id};
use crate::mapping::{
    agent_stage, find_operation, library, operation_dto, operation_error, projection_error,
};
use crate::protocol::{
    CliError, FeedEpisodeDto, FeedPodcastDto, MessageDto, ResponseData, bounded_page_limit,
};

impl Shell {
    pub(super) fn subscribe_feed(
        &mut self,
        feed_url: String,
        offset: u32,
        requested_limit: u16,
    ) -> Result<ResponseData, CliError> {
        let limit = bounded_page_limit(requested_limit);
        let (command_id, cancellation_id) = self.ids.command();
        let facade = Arc::clone(self.facade()?);
        facade.dispatch(CommandEnvelope {
            command_id,
            cancellation_id,
            expected_revision: None,
            command: ApplicationCommand::SubscribeToFeed { feed_url },
        });
        self.run_host_loop()?;
        let (library, totals, subscriptions) = library(&facade, offset, limit)?;
        let operation = find_operation(&library.operations, command_id)?;
        let (podcast_total, subscription_total, episode_total) = totals;
        let subscriptions: std::collections::BTreeSet<_> = subscriptions.into_iter().collect();
        Ok(ResponseData::FeedSubscription {
            command_id: encode_id(command_id.into_bytes()),
            operation: operation_dto(operation),
            offset,
            limit,
            has_more: library.has_more,
            podcast_total,
            subscription_total,
            episode_total,
            podcasts: library
                .podcasts
                .into_iter()
                .map(|podcast| FeedPodcastDto {
                    podcast_id: encode_id(podcast.podcast_id.into_bytes()),
                    title: podcast.title,
                    author: podcast.author,
                    feed_url: podcast.feed_identity.map(|identity| identity.source_url),
                    subscribed: subscriptions.contains(&podcast.podcast_id),
                })
                .collect(),
            episodes: library
                .episodes
                .into_iter()
                .map(|episode| FeedEpisodeDto {
                    episode_id: encode_id(episode.episode_id.into_bytes()),
                    podcast_id: encode_id(episode.podcast_id.into_bytes()),
                    publisher_guid: episode.publisher_guid,
                    title: episode.title,
                    published_at_milliseconds: episode.published_at.value,
                    enclosure_url: episode.enclosure_url,
                })
                .collect(),
        })
    }

    pub(super) fn set_setting(
        &mut self,
        setting: crate::protocol::SettingMutation,
        offset: u32,
        limit: u16,
    ) -> Result<ResponseData, CliError> {
        let facade = Arc::clone(self.facade()?);
        let command = crate::settings::command(&facade, setting)?;
        let (command_id, cancellation_id) = self.ids.command();
        facade.dispatch(CommandEnvelope {
            command_id,
            cancellation_id,
            expected_revision: None,
            command,
        });
        let (library, _, _) = library(&facade, 0, 1)?;
        let operation = find_operation(&library.operations, command_id)?;
        Ok(ResponseData::Settings {
            value: Box::new(crate::settings::read(&facade, offset, limit)?),
            operation: Some(operation_dto(operation)),
        })
    }

    pub(super) fn ask_agent(
        &mut self,
        input: String,
        conversation_id: Option<String>,
        provider: Option<crate::protocol::AgentProvider>,
        model: Option<String>,
    ) -> Result<ResponseData, CliError> {
        let target = self.host.config().resolve_model(provider, model)?;
        let conversation_id = conversation_id
            .as_deref()
            .map(|value| {
                parse_conversation_id(value).ok_or_else(|| {
                    CliError::new(
                        "invalid_id",
                        "conversation_id must be 32 hex characters",
                        false,
                    )
                })
            })
            .transpose()?;
        let facade = Arc::clone(self.facade()?);
        let (command_id, cancellation_id) = self.ids.command();
        facade.dispatch(CommandEnvelope {
            command_id,
            cancellation_id,
            expected_revision: None,
            command: ApplicationCommand::StartAgentTurn {
                conversation_id,
                user_input: input,
                model_reference: target.model_reference,
            },
        });
        let (projected_library, _, _) = library(&facade, 0, 1)?;
        let operation = find_operation(&projected_library.operations, command_id)?;
        let (conversation_id, turn_id) = match operation.result {
            Some(OperationResult::AgentTurnStarted {
                conversation_id,
                turn_id,
            }) => (conversation_id, turn_id),
            _ => return Err(operation_error(operation)),
        };
        self.run_host_loop()?;
        let conversation = match facade
            .snapshot(ProjectionRequest {
                scope: ProjectionScope::AgentConversation { conversation_id },
                offset: 0,
                max_items: 64,
            })
            .projection
        {
            Projection::AgentConversation { value } => value,
            _ => return Err(projection_error()),
        };
        let turn = conversation
            .turns
            .into_iter()
            .find(|value| value.turn_id == turn_id)
            .ok_or_else(|| {
                CliError::new("agent_turn_missing", "agent turn was not projected", true)
            })?;
        Ok(ResponseData::Agent {
            conversation_id: encode_id(conversation_id.into_bytes()),
            turn_id: encode_id(turn_id.into_bytes()),
            stage: agent_stage(turn.stage).to_owned(),
            messages: turn
                .messages
                .into_iter()
                .map(|message| MessageDto {
                    role: match message.role {
                        AgentMessageRole::User => "user",
                        AgentMessageRole::Assistant => "assistant",
                        AgentMessageRole::Tool => "tool",
                        AgentMessageRole::Error => "error",
                    }
                    .to_owned(),
                    content: message.content,
                })
                .collect(),
            safe_failure: turn.safe_failure,
        })
    }

    pub(super) fn host_drain(&self, _limit: u16) -> Result<ResponseData, CliError> {
        self.run_host_loop()?;
        Ok(ResponseData::HostDrain {
            pending: Vec::new(),
        })
    }

    pub(super) fn search_podcasts(
        &self,
        term: String,
        limit: u16,
    ) -> Result<ResponseData, CliError> {
        let results = crate::host::search::search(&self.host, &term, limit)?;
        Ok(ResponseData::PodcastSearch { results })
    }

    pub(super) fn library_page(
        &self,
        offset: u32,
        requested_limit: u16,
    ) -> Result<ResponseData, CliError> {
        let limit = bounded_page_limit(requested_limit);
        let facade = Arc::clone(self.facade()?);
        let (library, totals, subscribed_podcast_ids) = library(&facade, offset, limit)?;
        let (podcast_total, subscription_total, episode_total) = totals;
        let subscriptions: std::collections::BTreeSet<_> =
            subscribed_podcast_ids.into_iter().collect();
        Ok(ResponseData::Library {
            offset,
            limit,
            has_more: library.has_more,
            podcast_total,
            subscription_total,
            episode_total,
            podcasts: library
                .podcasts
                .into_iter()
                .map(|podcast| FeedPodcastDto {
                    podcast_id: encode_id(podcast.podcast_id.into_bytes()),
                    title: podcast.title,
                    author: podcast.author,
                    feed_url: podcast.feed_identity.map(|identity| identity.source_url),
                    subscribed: subscriptions.contains(&podcast.podcast_id),
                })
                .collect(),
            episodes: library
                .episodes
                .into_iter()
                .map(|episode| FeedEpisodeDto {
                    episode_id: encode_id(episode.episode_id.into_bytes()),
                    podcast_id: encode_id(episode.podcast_id.into_bytes()),
                    publisher_guid: episode.publisher_guid,
                    title: episode.title,
                    published_at_milliseconds: episode.published_at.value,
                    enclosure_url: episode.enclosure_url,
                })
                .collect(),
        })
    }

    pub(super) fn play_episode(&mut self, episode_id: String) -> Result<ResponseData, CliError> {
        let episode_id = parse_episode_id(&episode_id).ok_or_else(|| {
            CliError::new(
                "invalid_episode_id",
                "episode id must be 32 hex digits",
                false,
            )
        })?;
        self.dispatch_playback(PlaybackCommand::Select {
            episode_id,
            segment: None,
            label: None,
        })
    }

    pub(super) fn pause_playback(&mut self) -> Result<ResponseData, CliError> {
        self.dispatch_playback(PlaybackCommand::Pause)
    }

    pub(super) fn resume_playback(&mut self) -> Result<ResponseData, CliError> {
        self.dispatch_playback(PlaybackCommand::Play {
            transcript_configuration: None,
        })
    }

    pub(super) fn dispatch_playback(
        &mut self,
        command: PlaybackCommand,
    ) -> Result<ResponseData, CliError> {
        let (command_id, cancellation_id) = self.ids.command();
        let facade = Arc::clone(self.facade()?);
        facade.dispatch(CommandEnvelope {
            command_id,
            cancellation_id,
            expected_revision: None,
            command: ApplicationCommand::Playback { command },
        });
        self.run_host_loop()?;
        let (library, _, _) = library(&facade, 0, 1)?;
        let operation = find_operation(&library.operations, command_id)?;
        if operation.failure.is_some() {
            return Err(operation_error(operation));
        }
        Ok(ResponseData::Playback {
            command_id: encode_id(command_id.into_bytes()),
            operation: operation_dto(operation),
        })
    }
}
