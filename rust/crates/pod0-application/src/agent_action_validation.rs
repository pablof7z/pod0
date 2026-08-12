use crate::agent_policy::{
    AgentActionValidationError, validate_agent_model_reference, validate_optional_color_hex,
    validate_optional_text, validate_text,
};
use crate::agent_policy_shape::{
    episode_tool, no_argument_tool, podcast_tool, search_tool, text_tool,
};
use crate::{
    AgentToolAction, MAX_AGENT_ACTION_TEXT_BYTES, MAX_AGENT_CATEGORY_DESCRIPTION_BYTES,
    MAX_AGENT_CATEGORY_NAME_BYTES, MAX_CATEGORY_TAG_ITEMS,
};

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
            ..
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
        AgentToolAction::WriteCategory {
            category_id,
            name,
            description,
            color_hex,
            delete,
        } => {
            if *delete {
                // Delete addresses an existing category and cannot carry an
                // edit; anything else is an incoherent request.
                if category_id.is_none()
                    || name.is_some()
                    || description.is_some()
                    || color_hex.is_some()
                {
                    return Err(AgentActionValidationError::InvalidShape);
                }
                return Ok(());
            }
            // Creating requires a name and a description — an unnamed or
            // undescribed category is not navigable, and the description is
            // what the user reads on the category's own screen.
            if category_id.is_none() && (name.is_none() || description.is_none()) {
                return Err(AgentActionValidationError::InvalidShape);
            }
            // An edit that changes nothing is a wasted turn, not a no-op.
            if category_id.is_some()
                && name.is_none()
                && description.is_none()
                && color_hex.is_none()
            {
                return Err(AgentActionValidationError::InvalidShape);
            }
            if let Some(name) = name {
                validate_text(name, MAX_AGENT_CATEGORY_NAME_BYTES)?;
            }
            if let Some(description) = description {
                validate_text(description, MAX_AGENT_CATEGORY_DESCRIPTION_BYTES)?;
            }
            validate_optional_color_hex(color_hex.as_deref())
        }
        AgentToolAction::TagItems {
            add_item_ids,
            remove_item_ids,
            ..
        } => {
            if add_item_ids.is_empty() && remove_item_ids.is_empty() {
                return Err(AgentActionValidationError::InvalidShape);
            }
            if add_item_ids.len() > usize::from(MAX_CATEGORY_TAG_ITEMS)
                || remove_item_ids.len() > usize::from(MAX_CATEGORY_TAG_ITEMS)
            {
                return Err(AgentActionValidationError::ValueTooLarge);
            }
            // Adding and removing the same item in one call has no defined
            // resolution order, so reject rather than pick one silently.
            if add_item_ids
                .iter()
                .any(|item| remove_item_ids.contains(item))
            {
                return Err(AgentActionValidationError::InvalidShape);
            }
            Ok(())
        }
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
