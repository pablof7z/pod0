use pod0_domain::{ContentDigest, ProductSettingsValues};
use sha2::{Digest as _, Sha256};

pub const MAX_SETTING_TEXT_BYTES: usize = 256;
pub const MAX_SETTING_URL_BYTES: usize = 2_048;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SettingsField {
    ModelReference,
    ModelDisplayName,
    OllamaChatUrl,
    YoutubeExtractorUrl,
    ElevenLabsVoice,
    PlaybackRate,
    SkipInterval,
    AgentDisplayName,
    AgentAvatarUrl,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SettingsValidationCode {
    UnsupportedSchema,
    Empty,
    TooLong,
    InvalidUrl,
    OutOfRange,
    ControlCharacter,
    VersionReuse,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SettingsValidationState {
    Valid,
    Rejected {
        field: Option<SettingsField>,
        code: SettingsValidationCode,
    },
}

pub fn validate_product_settings(values: &ProductSettingsValues) -> SettingsValidationState {
    let models = [
        &values.agent_initial_model,
        &values.agent_thinking_model,
        &values.memory_compilation_model,
        &values.utility_model,
        &values.categorization_model,
        &values.chapter_compilation_model,
        &values.image_generation_model,
        &values.open_router_whisper_model,
        &values.assembly_ai_stt_model,
        &values.eleven_labs_stt_model,
        &values.eleven_labs_tts_model,
    ];
    for value in models {
        if let Some(code) = required_text_failure(value, MAX_SETTING_TEXT_BYTES) {
            return rejected(SettingsField::ModelReference, code);
        }
    }
    let names = [
        &values.agent_initial_model_name,
        &values.agent_thinking_model_name,
        &values.memory_compilation_model_name,
        &values.utility_model_name,
        &values.categorization_model_name,
        &values.chapter_compilation_model_name,
        &values.image_generation_model_name,
    ];
    for value in names {
        if let Some(code) = optional_text_failure(value, MAX_SETTING_TEXT_BYTES) {
            return rejected(SettingsField::ModelDisplayName, code);
        }
    }
    if !valid_url(&values.ollama_chat_url, &["http", "https"], false) {
        return rejected(
            SettingsField::OllamaChatUrl,
            SettingsValidationCode::InvalidUrl,
        );
    }
    if values
        .youtube_extractor_url
        .as_ref()
        .is_some_and(|value| !valid_url(value, &["http", "https"], false))
    {
        return rejected(
            SettingsField::YoutubeExtractorUrl,
            SettingsValidationCode::InvalidUrl,
        );
    }
    for value in [&values.eleven_labs_voice_id, &values.eleven_labs_voice_name] {
        if let Some(code) = optional_text_failure(value, MAX_SETTING_TEXT_BYTES) {
            return rejected(SettingsField::ElevenLabsVoice, code);
        }
    }
    if !(500..=3_000).contains(&values.default_playback_rate_milli) {
        return rejected(
            SettingsField::PlaybackRate,
            SettingsValidationCode::OutOfRange,
        );
    }
    if !(1..=3_600).contains(&values.skip_forward_seconds)
        || !(1..=3_600).contains(&values.skip_backward_seconds)
    {
        return rejected(
            SettingsField::SkipInterval,
            SettingsValidationCode::OutOfRange,
        );
    }
    if let Some(code) = optional_text_failure(&values.agent_display_name, MAX_SETTING_TEXT_BYTES) {
        return rejected(SettingsField::AgentDisplayName, code);
    }
    if values
        .agent_avatar_url
        .as_ref()
        .is_some_and(|value| !valid_url(value, &["http", "https"], false))
    {
        return rejected(
            SettingsField::AgentAvatarUrl,
            SettingsValidationCode::InvalidUrl,
        );
    }
    SettingsValidationState::Valid
}

pub(crate) fn settings_values_digest(values: &ProductSettingsValues) -> ContentDigest {
    let bytes = serde_json::to_vec(values).expect("settings values serialize");
    ContentDigest::from_bytes(Sha256::digest(bytes).into())
}

fn rejected(field: SettingsField, code: SettingsValidationCode) -> SettingsValidationState {
    SettingsValidationState::Rejected {
        field: Some(field),
        code,
    }
}

fn required_text_failure(value: &str, max: usize) -> Option<SettingsValidationCode> {
    if value.is_empty() || value.trim() != value {
        Some(SettingsValidationCode::Empty)
    } else {
        optional_text_failure(value, max)
    }
}

fn optional_text_failure(value: &str, max: usize) -> Option<SettingsValidationCode> {
    if value.len() > max {
        Some(SettingsValidationCode::TooLong)
    } else if value.chars().any(char::is_control) {
        Some(SettingsValidationCode::ControlCharacter)
    } else {
        None
    }
}

fn valid_url(value: &str, schemes: &[&str], allow_credentials: bool) -> bool {
    value.len() <= MAX_SETTING_URL_BYTES
        && url::Url::parse(value).is_ok_and(|url| {
            schemes.contains(&url.scheme())
                && url.host_str().is_some()
                && (allow_credentials || (url.username().is_empty() && url.password().is_none()))
        })
}
