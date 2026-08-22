mod host_loop;
mod store;

use std::sync::Arc;

use pod0_facade::{
    AgentMessageRole, ApplicationCommand, CommandEnvelope, FACADE_CONTRACT_VERSION,
    OperationResult, PlaybackCommand, Pod0Facade, Projection, ProjectionRequest,
    ProjectionScope,
};

use crate::app::host_loop::HostPump;
use crate::host::{HostConfig, HostExecutor};
use crate::ids::{IdFactory, encode_id, parse_conversation_id, parse_episode_id};
use crate::mapping::{
    agent_stage, find_operation, library, operation_dto, operation_error, projection_error,
};
use crate::protocol::{
    CliCommand, CliError, CliRequest, CliResponse, FeedEpisodeDto, FeedPodcastDto, MessageDto,
    PROTOCOL_VERSION, ResponseData, bounded_page_limit,
};

pub struct Shell {
    facade: Option<Arc<Pod0Facade>>,
    store_path: Option<String>,
    host: Arc<HostExecutor>,
    host_pump: Option<HostPump>,
    ids: IdFactory,
    exiting: bool,
}

impl Shell {
    pub fn new(config: HostConfig) -> Result<Self, CliError> {
        Ok(Self {
            facade: None,
            store_path: None,
            host: Arc::new(HostExecutor::new(config)?),
            host_pump: None,
            ids: IdFactory::new(),
            exiting: false,
        })
    }

    pub fn handle(&mut self, request: CliRequest) -> CliResponse {
        let request_id = request.request_id.clone();
        if request.version != PROTOCOL_VERSION {
            return CliResponse::failure(
                request_id,
                CliError::new(
                    "unsupported_protocol",
                    format!("protocol version {} is unsupported", request.version),
                    false,
                ),
            );
        }
        match self.execute(request.command) {
            Ok(result) => CliResponse::success(request_id, result),
            Err(error) => CliResponse::failure(request_id, error),
        }
    }

    #[must_use]
    pub const fn exiting(&self) -> bool {
        self.exiting
    }

    fn execute(&mut self, command: CliCommand) -> Result<ResponseData, CliError> {
        match command {
            CliCommand::Status => Ok(ResponseData::Status {
                facade_contract_version: FACADE_CONTRACT_VERSION,
                store_path: self.store_path.clone(),
                capabilities: self.host.config().capabilities(),
            }),
            CliCommand::Help => Ok(ResponseData::Help {
                commands: vec![
                    "status",
                    "help",
                    "create_store",
                    "open_store",
                    "subscribe_feed",
                    "search_podcasts",
                    "settings_get",
                    "settings_set",
                    "ask_agent",
                    "host_drain",
                    "library",
                    "play",
                    "pause",
                    "resume",
                    "exit",
                ],
            }),
            CliCommand::CreateStore { path } => self.create_store(path),
            CliCommand::OpenStore { path } => self.open_store(path),
            CliCommand::SubscribeFeed {
                feed_url,
                offset,
                limit,
            } => self.subscribe_feed(feed_url, offset, limit),
            CliCommand::SearchPodcasts { term, limit } => self.search_podcasts(term, limit),
            CliCommand::SettingsGet { offset, limit } => Ok(ResponseData::Settings {
                value: Box::new(crate::settings::read(self.facade()?, offset, limit)?),
                operation: None,
            }),
            CliCommand::SettingsSet {
                setting,
                offset,
                limit,
            } => self.set_setting(setting, offset, limit),
            CliCommand::AskAgent {
                input,
                conversation_id,
                provider,
                model,
            } => self.ask_agent(input, conversation_id, provider, model),
            CliCommand::HostDrain { limit } => self.host_drain(limit),
            CliCommand::Library { offset, limit } => self.library_page(offset, limit),
            CliCommand::Play { episode_id } => self.play_episode(episode_id),
            CliCommand::Pause => self.pause_playback(),
            CliCommand::Resume => self.resume_playback(),
            CliCommand::Exit => {
                self.exiting = true;
                self.stop_host_pump();
                Ok(ResponseData::Exit)
            }
        }
    }

    fn subscribe_feed(
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

    fn set_setting(
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

    fn ask_agent(
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

    fn host_drain(&self, _limit: u16) -> Result<ResponseData, CliError> {
        self.run_host_loop()?;
        Ok(ResponseData::HostDrain {
            pending: Vec::new(),
        })
    }

    fn search_podcasts(&self, term: String, limit: u16) -> Result<ResponseData, CliError> {
        let results = crate::host::search::search(&self.host, &term, limit)?;
        Ok(ResponseData::PodcastSearch { results })
    }

    fn library_page(&self, offset: u32, requested_limit: u16) -> Result<ResponseData, CliError> {
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

    fn play_episode(&mut self, episode_id: String) -> Result<ResponseData, CliError> {
        let episode_id = parse_episode_id(&episode_id).ok_or_else(|| {
            CliError::new("invalid_episode_id", "episode id must be 32 hex digits", false)
        })?;
        self.dispatch_playback(PlaybackCommand::Select {
            episode_id,
            segment: None,
            label: None,
        })
    }

    fn pause_playback(&mut self) -> Result<ResponseData, CliError> {
        self.dispatch_playback(PlaybackCommand::Pause)
    }

    fn resume_playback(&mut self) -> Result<ResponseData, CliError> {
        self.dispatch_playback(PlaybackCommand::Play {
            transcript_configuration: None,
        })
    }

    fn dispatch_playback(&mut self, command: PlaybackCommand) -> Result<ResponseData, CliError> {
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
