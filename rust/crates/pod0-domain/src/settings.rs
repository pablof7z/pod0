use crate::{ContentDigest, StateRevision};

pub const PRODUCT_SETTINGS_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, uniffi::Enum)]
pub enum HeadphoneGestureSetting {
    SkipForward,
    SkipBackward,
    NextChapter,
    PreviousChapter,
    ClipNow,
    None,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, uniffi::Enum)]
pub enum SpeechTranscriptionSetting {
    ElevenLabsScribe,
    AssemblyAi,
    OpenRouterWhisper,
    AppleNative,
}

/// Portable product preferences. Secret material and legacy-only migration
/// fields are deliberately absent from this type.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, uniffi::Record)]
pub struct ProductSettingsValues {
    pub agent_initial_model: String,
    pub agent_initial_model_name: String,
    pub agent_thinking_model: String,
    pub agent_thinking_model_name: String,
    pub memory_compilation_model: String,
    pub memory_compilation_model_name: String,
    pub utility_model: String,
    pub utility_model_name: String,
    pub categorization_model: String,
    pub categorization_model_name: String,
    pub chapter_compilation_model: String,
    pub chapter_compilation_model_name: String,
    pub image_generation_model: String,
    pub image_generation_model_name: String,
    pub ollama_chat_url: String,
    pub youtube_extractor_url: Option<String>,
    pub transcription_provider: SpeechTranscriptionSetting,
    pub open_router_whisper_model: String,
    pub assembly_ai_stt_model: String,
    pub eleven_labs_stt_model: String,
    pub eleven_labs_tts_model: String,
    pub eleven_labs_voice_id: String,
    pub eleven_labs_voice_name: String,
    pub default_playback_rate_milli: u16,
    pub skip_forward_seconds: u16,
    pub skip_backward_seconds: u16,
    pub auto_mark_played_at_end: bool,
    pub auto_delete_downloads_after_played: bool,
    pub auto_play_next: bool,
    pub auto_skip_ads: bool,
    pub headphone_double_tap_action: HeadphoneGestureSetting,
    pub headphone_triple_tap_action: HeadphoneGestureSetting,
    pub auto_ingest_publisher_transcripts: bool,
    pub auto_fallback_to_scribe: bool,
    pub agent_display_name: String,
    pub agent_avatar_url: Option<String>,
}

impl Default for ProductSettingsValues {
    fn default() -> Self {
        let utility_model = "openai/gpt-4o-mini".to_owned();
        Self {
            agent_initial_model: utility_model.clone(),
            agent_initial_model_name: String::new(),
            agent_thinking_model: utility_model.clone(),
            agent_thinking_model_name: String::new(),
            memory_compilation_model: utility_model.clone(),
            memory_compilation_model_name: String::new(),
            utility_model: utility_model.clone(),
            utility_model_name: String::new(),
            categorization_model: utility_model.clone(),
            categorization_model_name: String::new(),
            chapter_compilation_model: utility_model,
            chapter_compilation_model_name: String::new(),
            image_generation_model: "google/gemini-2.5-flash-image".into(),
            image_generation_model_name: String::new(),
            ollama_chat_url: "https://ollama.com/api/chat".into(),
            youtube_extractor_url: None,
            transcription_provider: SpeechTranscriptionSetting::ElevenLabsScribe,
            open_router_whisper_model: "openai/whisper-1".into(),
            assembly_ai_stt_model: "universal-3-pro,universal-2".into(),
            eleven_labs_stt_model: "scribe_v1".into(),
            eleven_labs_tts_model: "eleven_turbo_v2_5".into(),
            eleven_labs_voice_id: String::new(),
            eleven_labs_voice_name: String::new(),
            default_playback_rate_milli: 1_000,
            skip_forward_seconds: 30,
            skip_backward_seconds: 15,
            auto_mark_played_at_end: true,
            auto_delete_downloads_after_played: false,
            auto_play_next: true,
            auto_skip_ads: false,
            headphone_double_tap_action: HeadphoneGestureSetting::SkipForward,
            headphone_triple_tap_action: HeadphoneGestureSetting::ClipNow,
            auto_ingest_publisher_transcripts: true,
            auto_fallback_to_scribe: true,
            agent_display_name: String::new(),
            agent_avatar_url: None,
        }
    }
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    serde::Serialize,
    serde::Deserialize,
    uniffi::Record,
)]
pub struct SettingsWriterVersion {
    pub counter: u64,
    pub writer_id: ContentDigest,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, uniffi::Record)]
pub struct ProductSettings {
    pub schema_version: u32,
    pub revision: StateRevision,
    pub writer_version: SettingsWriterVersion,
    pub values: ProductSettingsValues,
}
