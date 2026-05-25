use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum OpenRouterCredentialSource {
    #[default]
    None,
    Manual,
    Byok,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ElevenLabsCredentialSource {
    #[default]
    None,
    Manual,
    Byok,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum OllamaCredentialSource {
    #[default]
    None,
    Manual,
    Byok,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HeadphoneGestureAction {
    SkipForward,
    SkipBackward,
    NextChapter,
    PreviousChapter,
    ClipNow,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SttProvider {
    ElevenLabsScribe,
    AssemblyAi,
    OpenRouterWhisper,
    AppleNative,
}

/// Provider credential triple shared by OpenRouter, Ollama, and ElevenLabs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ProviderCredentialMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub byok_key_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub byok_key_label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connected_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    // AI / LLM models
    pub agent_initial_model: String,
    pub agent_initial_model_name: String,
    pub agent_thinking_model: String,
    pub agent_thinking_model_name: String,
    pub memory_compilation_model: String,
    pub memory_compilation_model_name: String,
    pub wiki_model: String,
    pub wiki_model_name: String,
    pub categorization_model: String,
    pub categorization_model_name: String,
    pub chapter_compilation_model: String,
    pub chapter_compilation_model_name: String,
    pub embeddings_model: String,
    pub embeddings_model_name: String,
    pub image_generation_model: String,
    pub image_generation_model_name: String,
    pub reranker_enabled: bool,

    // Blossom
    pub blossom_server_url: String,

    // OpenRouter credentials (secret stored in Keychain; only metadata here)
    pub open_router_credential_source: OpenRouterCredentialSource,
    pub open_router: ProviderCredentialMetadata,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub legacy_open_router_api_key: Option<String>,

    // Ollama credentials
    pub ollama_credential_source: OllamaCredentialSource,
    pub ollama: ProviderCredentialMetadata,
    pub ollama_chat_url: String,

    // YouTube ingestion
    #[serde(skip_serializing_if = "Option::is_none")]
    pub youtube_extractor_url: Option<String>,

    // ElevenLabs credentials
    pub eleven_labs_credential_source: ElevenLabsCredentialSource,
    pub eleven_labs: ProviderCredentialMetadata,

    // STT
    pub stt_provider: SttProvider,
    pub open_router_whisper_model: String,
    pub assembly_ai_stt_model: String,

    // ElevenLabs config
    pub eleven_labs_stt_model: String,
    pub eleven_labs_tts_model: String,
    pub eleven_labs_voice_id: String,
    pub eleven_labs_voice_name: String,

    // Playback
    pub default_playback_rate: f64,
    pub skip_forward_secs: u32,
    pub skip_backward_secs: u32,
    pub auto_mark_played_at_end: bool,
    pub auto_delete_downloads_after_played: bool,
    pub auto_play_next: bool,
    pub auto_skip_ads: bool,
    pub headphone_double_tap_action: HeadphoneGestureAction,
    pub headphone_triple_tap_action: HeadphoneGestureAction,

    // Wiki
    pub wiki_auto_generate_on_transcript_ingest: bool,

    // Transcripts
    pub auto_ingest_publisher_transcripts: bool,
    pub auto_fallback_to_scribe: bool,

    // Notifications
    pub notify_on_new_episodes: bool,
    pub notify_on_briefing_ready: bool,

    // Nostr
    pub nostr_enabled: bool,
    pub nostr_relay_url: String,
    pub nostr_public_relays: Vec<String>,
    pub nostr_profile_name: String,
    pub nostr_profile_about: String,
    pub nostr_profile_picture: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nostr_public_key_hex: Option<String>,

    // Onboarding
    pub has_completed_onboarding: bool,
}

impl Settings {
    pub const DEFAULT_LLM_MODEL: &'static str = "openai/gpt-4o-mini";
    pub const DEFAULT_OLLAMA_CHAT_URL: &'static str = "https://ollama.com/api/chat";
    pub const DEFAULT_BLOSSOM_SERVER_URL: &'static str = "https://blossom.primal.net";
    pub const DEFAULT_NOSTR_RELAY_URL: &'static str = "wss://relay.tenex.chat";
    pub const DEFAULT_IMAGE_MODEL: &'static str = "google/gemini-2.5-flash-image";
    pub const DEFAULT_ELEVENLABS_STT_MODEL: &'static str = "scribe_v1";
    pub const DEFAULT_ELEVENLABS_TTS_MODEL: &'static str = "eleven_turbo_v2_5";
    pub const DEFAULT_WHISPER_MODEL: &'static str = "openai/whisper-1";
    pub const DEFAULT_ASSEMBLY_AI_MODEL: &'static str = "universal-3-pro,universal-2";
    // TODO(M6): pull from podcast-knowledge::settings once that crate lands.
    pub const DEFAULT_EMBEDDINGS_MODEL: &'static str = "openai/text-embedding-3-small";
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            agent_initial_model: Self::DEFAULT_LLM_MODEL.into(),
            agent_initial_model_name: String::new(),
            agent_thinking_model: Self::DEFAULT_LLM_MODEL.into(),
            agent_thinking_model_name: String::new(),
            memory_compilation_model: Self::DEFAULT_LLM_MODEL.into(),
            memory_compilation_model_name: String::new(),
            wiki_model: Self::DEFAULT_LLM_MODEL.into(),
            wiki_model_name: String::new(),
            categorization_model: Self::DEFAULT_LLM_MODEL.into(),
            categorization_model_name: String::new(),
            chapter_compilation_model: Self::DEFAULT_LLM_MODEL.into(),
            chapter_compilation_model_name: String::new(),
            embeddings_model: Self::DEFAULT_EMBEDDINGS_MODEL.into(),
            embeddings_model_name: String::new(),
            image_generation_model: Self::DEFAULT_IMAGE_MODEL.into(),
            image_generation_model_name: String::new(),
            reranker_enabled: false,

            blossom_server_url: Self::DEFAULT_BLOSSOM_SERVER_URL.into(),

            open_router_credential_source: OpenRouterCredentialSource::None,
            open_router: ProviderCredentialMetadata::default(),
            legacy_open_router_api_key: None,

            ollama_credential_source: OllamaCredentialSource::None,
            ollama: ProviderCredentialMetadata::default(),
            ollama_chat_url: Self::DEFAULT_OLLAMA_CHAT_URL.into(),

            youtube_extractor_url: None,

            eleven_labs_credential_source: ElevenLabsCredentialSource::None,
            eleven_labs: ProviderCredentialMetadata::default(),

            stt_provider: SttProvider::ElevenLabsScribe,
            open_router_whisper_model: Self::DEFAULT_WHISPER_MODEL.into(),
            assembly_ai_stt_model: Self::DEFAULT_ASSEMBLY_AI_MODEL.into(),

            eleven_labs_stt_model: Self::DEFAULT_ELEVENLABS_STT_MODEL.into(),
            eleven_labs_tts_model: Self::DEFAULT_ELEVENLABS_TTS_MODEL.into(),
            eleven_labs_voice_id: String::new(),
            eleven_labs_voice_name: String::new(),

            default_playback_rate: 1.0,
            skip_forward_secs: 30,
            skip_backward_secs: 15,
            auto_mark_played_at_end: true,
            auto_delete_downloads_after_played: false,
            auto_play_next: true,
            auto_skip_ads: false,
            headphone_double_tap_action: HeadphoneGestureAction::SkipForward,
            headphone_triple_tap_action: HeadphoneGestureAction::ClipNow,

            wiki_auto_generate_on_transcript_ingest: false,

            auto_ingest_publisher_transcripts: true,
            auto_fallback_to_scribe: true,

            notify_on_new_episodes: true,
            notify_on_briefing_ready: true,

            nostr_enabled: false,
            nostr_relay_url: Self::DEFAULT_NOSTR_RELAY_URL.into(),
            nostr_public_relays: Vec::new(),
            nostr_profile_name: String::new(),
            nostr_profile_about: String::new(),
            nostr_profile_picture: String::new(),
            nostr_public_key_hex: None,

            has_completed_onboarding: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_default_round_trip() {
        let value = Settings::default();
        let json = serde_json::to_string(&value).unwrap();
        let back: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(value, back);
    }

    #[test]
    fn settings_with_credentials_round_trip() {
        let mut value = Settings::default();
        value.open_router_credential_source = OpenRouterCredentialSource::Byok;
        value.open_router.byok_key_id = Some("k1".into());
        value.open_router.byok_key_label = Some("Personal".into());
        let json = serde_json::to_string(&value).unwrap();
        let back: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(value, back);
    }
}
