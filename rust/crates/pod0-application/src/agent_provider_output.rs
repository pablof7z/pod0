use pod0_domain::{EpisodeId, PodcastId};
use serde_json::{Map, Value};

use crate::{AgentToolAction, AgentToolName, QueuePlacement, agent_tool_name_from_wire};

pub const MAX_AGENT_TOOL_CALL_ID_BYTES: usize = 512;
pub const MAX_AGENT_TOOL_NAME_BYTES: usize = 128;
pub const MAX_AGENT_TOOL_ARGUMENTS_BYTES: usize = 64 * 1_024;

/// Bounded, untrusted provider output. Native transports do not interpret
/// tool arguments or construct an authorized domain action; the Rust kernel
/// parses this observation against its closed action schema.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct AgentModelToolCallObservation {
    pub provider_call_id: String,
    pub tool_name: String,
    pub arguments_json: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentProviderOutputError {
    UnsupportedTool,
    InvalidJson,
    InvalidArguments,
}

pub fn parse_agent_tool_call(
    call: &AgentModelToolCallObservation,
) -> Result<AgentToolAction, AgentProviderOutputError> {
    let tool = agent_tool_name_from_wire(&call.tool_name)
        .ok_or(AgentProviderOutputError::UnsupportedTool)?;
    let value: Value = serde_json::from_str(&call.arguments_json)
        .map_err(|_| AgentProviderOutputError::InvalidJson)?;
    let args = value
        .as_object()
        .ok_or(AgentProviderOutputError::InvalidArguments)?;
    parse_action(tool, args)
}

fn parse_action(
    tool: AgentToolName,
    args: &Map<String, Value>,
) -> Result<AgentToolAction, AgentProviderOutputError> {
    use AgentToolName::*;
    match tool {
        CreateNote => Ok(AgentToolAction::CreateNote {
            text: required_text(args, "text")?,
        }),
        UseSkill => Ok(AgentToolAction::TextInput {
            tool,
            text: required_text(args, "skill_id")?,
        }),
        ListSubscriptions | ListPodcasts | ListInProgress | ListRecentUnplayed | PausePlayback => {
            Ok(AgentToolAction::NoArguments { tool })
        }
        SearchEpisodes => Ok(AgentToolAction::Search {
            tool,
            query: required_text(args, "query")?,
            scope: optional_text(args, "scope")?,
            limit: optional_u16(args, "limit")?.unwrap_or(10),
        }),
        ListEpisodes => Ok(AgentToolAction::Podcast {
            tool,
            podcast_id: opaque_id(args, "podcast_id", PodcastId::from_bytes)?,
        }),
        PlayEpisode => Ok(AgentToolAction::PlayEpisode {
            episode_id: opaque_id(args, "episode_id", EpisodeId::from_bytes)?,
            start_milliseconds: optional_seconds(args, "start_seconds")?,
            end_milliseconds: optional_seconds(args, "end_seconds")?,
            placement: queue_placement(args)?,
        }),
        SetPlaybackRate => {
            let rate = required_number(args, "rate")?;
            let permille = (rate * 1_000.0).round();
            if !(0.0..=f64::from(u16::MAX)).contains(&permille) {
                return Err(AgentProviderOutputError::InvalidArguments);
            }
            Ok(AgentToolAction::SetPlaybackRate {
                permille: permille as u16,
            })
        }
        SetSleepTimer => Ok(AgentToolAction::SetSleepTimer {
            duration_milliseconds: sleep_timer_duration(args)?,
        }),
        _ => Err(AgentProviderOutputError::UnsupportedTool),
    }
}

fn required_text(
    args: &Map<String, Value>,
    name: &str,
) -> Result<String, AgentProviderOutputError> {
    args.get(name)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_owned)
        .ok_or(AgentProviderOutputError::InvalidArguments)
}

fn optional_text(
    args: &Map<String, Value>,
    name: &str,
) -> Result<Option<String>, AgentProviderOutputError> {
    match args.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) if value.trim().is_empty() => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        _ => Err(AgentProviderOutputError::InvalidArguments),
    }
}

fn required_number(args: &Map<String, Value>, name: &str) -> Result<f64, AgentProviderOutputError> {
    args.get(name)
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite())
        .ok_or(AgentProviderOutputError::InvalidArguments)
}

fn optional_u16(
    args: &Map<String, Value>,
    name: &str,
) -> Result<Option<u16>, AgentProviderOutputError> {
    match args.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_u64()
            .and_then(|number| u16::try_from(number).ok())
            .map(Some)
            .ok_or(AgentProviderOutputError::InvalidArguments),
    }
}

fn optional_seconds(
    args: &Map<String, Value>,
    name: &str,
) -> Result<Option<u64>, AgentProviderOutputError> {
    match args.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(_) => {
            let seconds = required_number(args, name)?;
            if seconds < 0.0 || seconds > u64::MAX as f64 / 1_000.0 {
                return Err(AgentProviderOutputError::InvalidArguments);
            }
            Ok(Some((seconds * 1_000.0).round() as u64))
        }
    }
}

fn opaque_id<T>(
    args: &Map<String, Value>,
    name: &str,
    constructor: impl FnOnce([u8; 16]) -> T,
) -> Result<T, AgentProviderOutputError> {
    let value = required_text(args, name)?;
    parse_uuid_bytes(&value)
        .map(constructor)
        .ok_or(AgentProviderOutputError::InvalidArguments)
}

fn parse_uuid_bytes(value: &str) -> Option<[u8; 16]> {
    let hex = value
        .chars()
        .filter(|character| *character != '-')
        .collect::<String>();
    if hex.len() != 32 {
        return None;
    }
    let mut bytes = [0_u8; 16];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16).ok()?;
    }
    Some(bytes)
}

fn queue_placement(args: &Map<String, Value>) -> Result<QueuePlacement, AgentProviderOutputError> {
    match args
        .get("queue_position")
        .and_then(Value::as_str)
        .unwrap_or("next")
    {
        "next" => Ok(QueuePlacement::Next),
        "end" => Ok(QueuePlacement::Back),
        _ => Err(AgentProviderOutputError::InvalidArguments),
    }
}

fn sleep_timer_duration(
    args: &Map<String, Value>,
) -> Result<Option<u64>, AgentProviderOutputError> {
    match args.get("mode").and_then(Value::as_str) {
        Some("off") => Ok(None),
        Some("minutes") => args
            .get("minutes")
            .and_then(Value::as_u64)
            .and_then(|minutes| minutes.checked_mul(60_000))
            .map(Some)
            .ok_or(AgentProviderOutputError::InvalidArguments),
        _ => Err(AgentProviderOutputError::InvalidArguments),
    }
}
