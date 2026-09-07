use std::collections::BTreeSet;

use pod0_domain::{HeadphoneGestureSetting, ProductSettingsValues, SpeechTranscriptionSetting};

pub const MAX_PRODUCT_SETTING_INTENTS: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, uniffi::Enum)]
pub enum ProductModelSlot {
    AgentInitial,
    AgentThinking,
    MemoryCompilation,
    Utility,
    Categorization,
    ChapterCompilation,
    ImageGeneration,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, uniffi::Enum)]
pub enum TranscriptionModelSlot {
    OpenRouterWhisper,
    AssemblyAi,
    ElevenLabs,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, uniffi::Enum)]
pub enum PlaybackSettingToggle {
    AutoMarkPlayedAtEnd,
    AutoDeleteDownloadsAfterPlayed,
    AutoPlayNext,
    AutoSkipAds,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, uniffi::Enum)]
pub enum TranscriptSettingToggle {
    AutoIngestPublisherTranscripts,
    AutoFallbackToScribe,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, uniffi::Enum)]
pub enum HeadphoneGestureTap {
    Double,
    Triple,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, uniffi::Enum)]
pub enum ProductSettingIntent {
    SelectModel {
        slot: ProductModelSlot,
        model_id: String,
        model_name: String,
    },
    SetOllamaChatUrl {
        url: String,
    },
    SetYoutubeExtractorUrl {
        url: Option<String>,
    },
    SetTranscriptionProvider {
        provider: SpeechTranscriptionSetting,
    },
    SetTranscriptionModel {
        slot: TranscriptionModelSlot,
        model_id: String,
    },
    SetTextToSpeechModel {
        model_id: String,
    },
    SetTextToSpeechVoice {
        voice_id: String,
        voice_name: String,
    },
    SetPlaybackRate {
        milli: u16,
    },
    SetSkipIntervals {
        forward_seconds: u16,
        backward_seconds: u16,
    },
    SetPlaybackToggle {
        setting: PlaybackSettingToggle,
        enabled: bool,
    },
    SetHeadphoneGesture {
        tap: HeadphoneGestureTap,
        action: HeadphoneGestureSetting,
    },
    SetTranscriptToggle {
        setting: TranscriptSettingToggle,
        enabled: bool,
    },
    SetAgentIdentity {
        display_name: String,
        avatar_url: Option<String>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProductSettingIntentError {
    Empty,
    TooMany,
    DuplicateTarget,
}

pub fn apply_product_setting_intents(
    current: &ProductSettingsValues,
    intents: &[ProductSettingIntent],
) -> Result<ProductSettingsValues, ProductSettingIntentError> {
    if intents.is_empty() {
        return Err(ProductSettingIntentError::Empty);
    }
    if intents.len() > MAX_PRODUCT_SETTING_INTENTS {
        return Err(ProductSettingIntentError::TooMany);
    }
    let mut targets = BTreeSet::new();
    let mut values = current.clone();
    for intent in intents {
        if !targets.insert(target(intent)) {
            return Err(ProductSettingIntentError::DuplicateTarget);
        }
        apply(&mut values, intent);
    }
    Ok(values)
}

fn target(intent: &ProductSettingIntent) -> (u8, u8) {
    match intent {
        ProductSettingIntent::SelectModel { slot, .. } => (0, *slot as u8),
        ProductSettingIntent::SetOllamaChatUrl { .. } => (1, 0),
        ProductSettingIntent::SetYoutubeExtractorUrl { .. } => (2, 0),
        ProductSettingIntent::SetTranscriptionProvider { .. } => (3, 0),
        ProductSettingIntent::SetTranscriptionModel { slot, .. } => (4, *slot as u8),
        ProductSettingIntent::SetTextToSpeechModel { .. } => (5, 0),
        ProductSettingIntent::SetTextToSpeechVoice { .. } => (6, 0),
        ProductSettingIntent::SetPlaybackRate { .. } => (7, 0),
        ProductSettingIntent::SetSkipIntervals { .. } => (8, 0),
        ProductSettingIntent::SetPlaybackToggle { setting, .. } => (9, *setting as u8),
        ProductSettingIntent::SetHeadphoneGesture { tap, .. } => (10, *tap as u8),
        ProductSettingIntent::SetTranscriptToggle { setting, .. } => (11, *setting as u8),
        ProductSettingIntent::SetAgentIdentity { .. } => (12, 0),
    }
}

fn apply(values: &mut ProductSettingsValues, intent: &ProductSettingIntent) {
    match intent {
        ProductSettingIntent::SelectModel {
            slot,
            model_id,
            model_name,
        } => {
            set_model(values, *slot, model_id, model_name);
        }
        ProductSettingIntent::SetOllamaChatUrl { url } => values.ollama_chat_url = url.clone(),
        ProductSettingIntent::SetYoutubeExtractorUrl { url } => {
            values.youtube_extractor_url = url.clone();
        }
        ProductSettingIntent::SetTranscriptionProvider { provider } => {
            values.transcription_provider = *provider;
        }
        ProductSettingIntent::SetTranscriptionModel { slot, model_id } => match slot {
            TranscriptionModelSlot::OpenRouterWhisper => {
                values.open_router_whisper_model = model_id.clone();
            }
            TranscriptionModelSlot::AssemblyAi => values.assembly_ai_stt_model = model_id.clone(),
            TranscriptionModelSlot::ElevenLabs => values.eleven_labs_stt_model = model_id.clone(),
        },
        ProductSettingIntent::SetTextToSpeechModel { model_id } => {
            values.eleven_labs_tts_model = model_id.clone();
        }
        ProductSettingIntent::SetTextToSpeechVoice {
            voice_id,
            voice_name,
        } => {
            values.eleven_labs_voice_id = voice_id.clone();
            values.eleven_labs_voice_name = voice_name.clone();
        }
        ProductSettingIntent::SetPlaybackRate { milli } => {
            values.default_playback_rate_milli = *milli;
        }
        ProductSettingIntent::SetSkipIntervals {
            forward_seconds,
            backward_seconds,
        } => {
            values.skip_forward_seconds = *forward_seconds;
            values.skip_backward_seconds = *backward_seconds;
        }
        ProductSettingIntent::SetPlaybackToggle { setting, enabled } => {
            set_playback_toggle(values, *setting, *enabled);
        }
        ProductSettingIntent::SetHeadphoneGesture { tap, action } => match tap {
            HeadphoneGestureTap::Double => values.headphone_double_tap_action = *action,
            HeadphoneGestureTap::Triple => values.headphone_triple_tap_action = *action,
        },
        ProductSettingIntent::SetTranscriptToggle { setting, enabled } => match setting {
            TranscriptSettingToggle::AutoIngestPublisherTranscripts => {
                values.auto_ingest_publisher_transcripts = *enabled;
            }
            TranscriptSettingToggle::AutoFallbackToScribe => {
                values.auto_fallback_to_scribe = *enabled;
            }
        },
        ProductSettingIntent::SetAgentIdentity {
            display_name,
            avatar_url,
        } => {
            values.agent_display_name = display_name.clone();
            values.agent_avatar_url = avatar_url.clone();
        }
    }
}

fn set_model(
    values: &mut ProductSettingsValues,
    slot: ProductModelSlot,
    model_id: &str,
    model_name: &str,
) {
    let pair = match slot {
        ProductModelSlot::AgentInitial => (
            &mut values.agent_initial_model,
            &mut values.agent_initial_model_name,
        ),
        ProductModelSlot::AgentThinking => (
            &mut values.agent_thinking_model,
            &mut values.agent_thinking_model_name,
        ),
        ProductModelSlot::MemoryCompilation => (
            &mut values.memory_compilation_model,
            &mut values.memory_compilation_model_name,
        ),
        ProductModelSlot::Utility => (&mut values.utility_model, &mut values.utility_model_name),
        ProductModelSlot::Categorization => (
            &mut values.categorization_model,
            &mut values.categorization_model_name,
        ),
        ProductModelSlot::ChapterCompilation => (
            &mut values.chapter_compilation_model,
            &mut values.chapter_compilation_model_name,
        ),
        ProductModelSlot::ImageGeneration => (
            &mut values.image_generation_model,
            &mut values.image_generation_model_name,
        ),
    };
    *pair.0 = model_id.to_owned();
    *pair.1 = model_name.to_owned();
}

fn set_playback_toggle(
    values: &mut ProductSettingsValues,
    setting: PlaybackSettingToggle,
    enabled: bool,
) {
    match setting {
        PlaybackSettingToggle::AutoMarkPlayedAtEnd => values.auto_mark_played_at_end = enabled,
        PlaybackSettingToggle::AutoDeleteDownloadsAfterPlayed => {
            values.auto_delete_downloads_after_played = enabled;
        }
        PlaybackSettingToggle::AutoPlayNext => values.auto_play_next = enabled,
        PlaybackSettingToggle::AutoSkipAds => values.auto_skip_ads = enabled,
    }
}
