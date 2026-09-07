mod commands;
mod host_loop;
mod store;

use std::sync::Arc;

use pod0_facade::{FACADE_CONTRACT_VERSION, Pod0Facade};

use crate::app::host_loop::HostPump;
use crate::host::{HostConfig, HostExecutor};
use crate::ids::IdFactory;
use crate::protocol::{
    CliCommand, CliError, CliRequest, CliResponse, PROTOCOL_VERSION, ResponseData,
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
}
