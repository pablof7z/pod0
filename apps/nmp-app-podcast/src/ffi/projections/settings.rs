use serde::{Deserialize, Serialize};

fn default_skip_forward_secs() -> f64 { 30.0 }
fn default_skip_backward_secs() -> f64 { 15.0 }
fn default_one() -> f64 { 1.0 }
fn default_true() -> bool { true }
fn default_skip_forward_action() -> String { "skipForward".to_owned() }
fn default_clip_now_action() -> String { "clipNow".to_owned() }
fn default_agent_initial_model() -> String { "deepseek-v4-flash:cloud".to_owned() }
fn default_agent_initial_model_name() -> String { "DeepSeek Flash".to_owned() }
fn default_agent_thinking_model() -> String { "deepseek-v4-pro:cloud".to_owned() }
fn default_agent_thinking_model_name() -> String { "DeepSeek Pro".to_owned() }
fn default_memory_compilation_model() -> String { "deepseek-v4-flash:cloud".to_owned() }
fn default_memory_compilation_model_name() -> String { "DeepSeek Flash".to_owned() }
fn default_wiki_model() -> String { "deepseek-v4-flash:cloud".to_owned() }
fn default_wiki_model_name() -> String { "DeepSeek Flash".to_owned() }
fn default_categorization_model() -> String { "deepseek-v4-flash:cloud".to_owned() }
fn default_categorization_model_name() -> String { "DeepSeek Flash".to_owned() }
fn default_chapter_compilation_model() -> String { "deepseek-v4-flash:cloud".to_owned() }
fn default_chapter_compilation_model_name() -> String { "DeepSeek Flash".to_owned() }
fn default_embeddings_model() -> String { "deepseek-v4-flash:cloud".to_owned() }
fn default_embeddings_model_name() -> String { "DeepSeek Flash".to_owned() }
fn default_image_generation_model() -> String { "google/gemini-2.5-flash-image".to_owned() }
fn default_image_generation_model_name() -> String { "Gemini 2.5 Flash".to_owned() }
fn default_false() -> bool { false }
fn default_empty_string() -> String { String::new() }
fn default_empty_array() -> Vec<String> { Vec::new() }
fn default_stt_provider() -> String { "apple_native".to_owned() }
fn default_open_router_whisper_model() -> String { "openai/whisper-1".to_owned() }
fn default_assembly_ai_stt_model() -> String { "universal-3-pro,universal-2".to_owned() }
fn default_eleven_labs_stt_model() -> String { "scribe_v1".to_owned() }
fn default_eleven_labs_tts_model() -> String { "eleven_turbo_v2_5".to_owned() }
fn default_blossom_server_url() -> String { "https://blossom.primal.net".to_owned() }

/// App-settings projection surfaced via
/// [`super::snapshot::PodcastUpdate::settings`].
///
/// Replaces the legacy in-memory `Settings` compat shim. The kernel
/// authoritative source is [`crate::store::PodcastStore`] accessors.
///
/// `Default` produces the fresh-install state so the snapshot builder can
/// always emit a `SettingsSnapshot` regardless of store-lock acquisition.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SettingsSnapshot {
    /// Whether the user has finished the iOS onboarding flow.
    #[serde(default)]
    pub has_completed_onboarding: bool,
    /// When `true`, the player actor seeks past each ad segment.
    #[serde(default)]
    pub auto_skip_ads_enabled: bool,
    /// When `true`, the kernel auto-advances to the next queued episode
    /// on `ItemEnd`. Default `true`.
    #[serde(default = "default_true")]
    pub auto_play_next: bool,
    /// When `true`, the kernel marks the episode listened on `ItemEnd`.
    /// Default `true`.
    #[serde(default = "default_true")]
    pub auto_mark_played_at_end: bool,
    /// Raw action string for headphone double-tap gesture. Default `"skip_forward"`.
    #[serde(default = "default_skip_forward_action")]
    pub headphone_double_tap_action: String,
    /// Raw action string for headphone triple-tap gesture. Default `"clip_now"`.
    #[serde(default = "default_clip_now_action")]
    pub headphone_triple_tap_action: String,
    /// Skip-forward interval in seconds. Default 30.0.
    #[serde(default = "default_skip_forward_secs")]
    pub skip_forward_secs: f64,
    /// Skip-backward interval in seconds. Default 15.0.
    #[serde(default = "default_skip_backward_secs")]
    pub skip_backward_secs: f64,
    /// Default playback rate. Default 1.0; range [0.5, 3.0].
    #[serde(default = "default_one")]
    pub default_playback_rate: f64,
    /// When `true`, downloaded files are deleted after the episode is marked played.
    #[serde(default)]
    pub auto_delete_downloads_after_played: bool,
    /// LLM model ID for initial agent chat. Default `"deepseek-v4-flash:cloud"`.
    #[serde(default = "default_agent_initial_model")]
    pub agent_initial_model: String,
    /// Human-readable name for initial agent model. Default `"DeepSeek Flash"`.
    #[serde(default = "default_agent_initial_model_name")]
    pub agent_initial_model_name: String,
    /// LLM model ID for agent thinking/planning. Default `"deepseek-v4-pro:cloud"`.
    #[serde(default = "default_agent_thinking_model")]
    pub agent_thinking_model: String,
    /// Human-readable name for agent thinking model. Default `"DeepSeek Pro"`.
    #[serde(default = "default_agent_thinking_model_name")]
    pub agent_thinking_model_name: String,
    /// LLM model ID for memory compilation. Default `"deepseek-v4-flash:cloud"`.
    #[serde(default = "default_memory_compilation_model")]
    pub memory_compilation_model: String,
    /// Human-readable name for memory compilation model. Default `"DeepSeek Flash"`.
    #[serde(default = "default_memory_compilation_model_name")]
    pub memory_compilation_model_name: String,
    /// LLM model ID for wiki synthesis. Default `"deepseek-v4-flash:cloud"`.
    #[serde(default = "default_wiki_model")]
    pub wiki_model: String,
    /// Human-readable name for wiki model. Default `"DeepSeek Flash"`.
    #[serde(default = "default_wiki_model_name")]
    pub wiki_model_name: String,
    /// LLM model ID for episode categorization. Default `"deepseek-v4-flash:cloud"`.
    #[serde(default = "default_categorization_model")]
    pub categorization_model: String,
    /// Human-readable name for categorization model. Default `"DeepSeek Flash"`.
    #[serde(default = "default_categorization_model_name")]
    pub categorization_model_name: String,
    /// LLM model ID for chapter compilation. Default `"deepseek-v4-flash:cloud"`.
    #[serde(default = "default_chapter_compilation_model")]
    pub chapter_compilation_model: String,
    /// Human-readable name for chapter compilation model. Default `"DeepSeek Flash"`.
    #[serde(default = "default_chapter_compilation_model_name")]
    pub chapter_compilation_model_name: String,
    /// LLM model ID for embeddings generation. Default `"deepseek-v4-flash:cloud"`.
    #[serde(default = "default_embeddings_model")]
    pub embeddings_model: String,
    /// Human-readable name for embeddings model. Default `"DeepSeek Flash"`.
    #[serde(default = "default_embeddings_model_name")]
    pub embeddings_model_name: String,
    /// LLM model ID for image generation. Default `"google/gemini-2.5-flash-image"`.
    #[serde(default = "default_image_generation_model")]
    pub image_generation_model: String,
    /// Human-readable name for image generation model. Default `"Gemini 2.5 Flash"`.
    #[serde(default = "default_image_generation_model_name")]
    pub image_generation_model_name: String,
    /// Whether the reranker is enabled for search results. Default `false`.
    #[serde(default = "default_false")]
    pub reranker_enabled: bool,
    /// OpenRouter credential source enum (raw String: "apiKey", "byok", "nostr").
    #[serde(default = "default_empty_string")]
    pub open_router_credential_source: String,
    /// OpenRouter BYOK key ID (optional).
    #[serde(default)]
    pub open_router_byok_key_id: Option<String>,
    /// OpenRouter BYOK key label (optional).
    #[serde(default)]
    pub open_router_byok_key_label: Option<String>,
    /// OpenRouter credential connected-at timestamp (epoch seconds, optional).
    #[serde(default)]
    pub open_router_connected_at: Option<i64>,
    /// Ollama credential source enum (raw String: "apiKey", "byok", "nostr").
    #[serde(default = "default_empty_string")]
    pub ollama_credential_source: String,
    /// Ollama BYOK key ID (optional).
    #[serde(default)]
    pub ollama_byok_key_id: Option<String>,
    /// Ollama BYOK key label (optional).
    #[serde(default)]
    pub ollama_byok_key_label: Option<String>,
    /// Ollama credential connected-at timestamp (epoch seconds, optional).
    #[serde(default)]
    pub ollama_connected_at: Option<i64>,
    /// Ollama chat endpoint URL for LLM inference.
    #[serde(default = "default_empty_string")]
    pub ollama_chat_url: String,
    /// ElevenLabs credential source enum (raw String: "apiKey", "byok", "nostr").
    #[serde(default = "default_empty_string")]
    pub eleven_labs_credential_source: String,
    /// ElevenLabs BYOK key ID (optional).
    #[serde(default)]
    pub eleven_labs_byok_key_id: Option<String>,
    /// ElevenLabs BYOK key label (optional).
    #[serde(default)]
    pub eleven_labs_byok_key_label: Option<String>,
    /// ElevenLabs credential connected-at timestamp (epoch seconds, optional).
    #[serde(default)]
    pub eleven_labs_connected_at: Option<i64>,
    /// STT provider selection enum (raw String: "apple_native", etc).
    #[serde(default = "default_stt_provider")]
    pub stt_provider: String,
    /// OpenRouter Whisper model string. Default `"openai/whisper-1"`.
    #[serde(default = "default_open_router_whisper_model")]
    pub open_router_whisper_model: String,
    /// AssemblyAI STT model string. Default `"universal-3-pro,universal-2"`.
    #[serde(default = "default_assembly_ai_stt_model")]
    pub assembly_ai_stt_model: String,
    /// ElevenLabs STT model string. Default `"scribe_v1"`.
    #[serde(default = "default_eleven_labs_stt_model")]
    pub eleven_labs_stt_model: String,
    /// ElevenLabs TTS model string. Default `"eleven_turbo_v2_5"`.
    #[serde(default = "default_eleven_labs_tts_model")]
    pub eleven_labs_tts_model: String,
    /// ElevenLabs voice ID. Defaults to empty string.
    #[serde(default)]
    pub eleven_labs_voice_id: String,
    /// ElevenLabs voice name. Defaults to empty string.
    #[serde(default)]
    pub eleven_labs_voice_name: String,
    /// Blossom server URL. Default `"https://blossom.primal.net"`.
    #[serde(default = "default_blossom_server_url")]
    pub blossom_server_url: String,
    /// YouTube extractor URL (optional).
    #[serde(default)]
    pub youtube_extractor_url: Option<String>,
    /// Whether to auto-generate wiki entries when transcripts are ingested. Default `false`.
    #[serde(default = "default_false")]
    pub wiki_auto_generate_on_transcript_ingest: bool,
    /// Whether to auto-ingest publisher-provided transcripts. Default `true`.
    #[serde(default = "default_true")]
    pub auto_ingest_publisher_transcripts: bool,
    /// Whether to fall back to Scribe (STT) when publisher transcript ingestion fails. Default `true`.
    #[serde(default = "default_true")]
    pub auto_fallback_to_scribe: bool,
    /// Whether to send local notifications when new episodes arrive. Default `true`.
    #[serde(default = "default_true")]
    pub notify_on_new_episodes: bool,
    /// Whether to send local notifications when briefing/AI processing is ready. Default `true`.
    #[serde(default = "default_true")]
    pub notify_on_briefing_ready: bool,
    /// Whether Nostr publishing and identity features are enabled. Default `false`.
    #[serde(default = "default_false")]
    pub nostr_enabled: bool,
    /// Primary Nostr relay URL for publishing and event distribution. Default empty.
    #[serde(default = "default_empty_string")]
    pub nostr_relay_url: String,
    /// List of public Nostr relay URLs for broadcast and subscription. Default empty.
    #[serde(default = "default_empty_array")]
    pub nostr_public_relays: Vec<String>,
    /// User's display name in Nostr profile metadata. Default empty.
    #[serde(default = "default_empty_string")]
    pub nostr_profile_name: String,
    /// User's about/bio text in Nostr profile metadata. Default empty.
    #[serde(default = "default_empty_string")]
    pub nostr_profile_about: String,
    /// User's picture URL in Nostr profile metadata. Default empty.
    #[serde(default = "default_empty_string")]
    pub nostr_profile_picture: String,
    /// Nostr public key hex (read-only, derived from Keychain). Not persisted.
    #[serde(default)]
    pub nostr_public_key_hex: Option<String>,
}

impl Default for SettingsSnapshot {
    fn default() -> Self {
        Self {
            has_completed_onboarding: false,
            auto_skip_ads_enabled: false,
            auto_play_next: true,
            auto_mark_played_at_end: true,
            headphone_double_tap_action: "skipForward".to_owned(),
            headphone_triple_tap_action: "clipNow".to_owned(),
            skip_forward_secs: 30.0,
            skip_backward_secs: 15.0,
            default_playback_rate: 1.0,
            auto_delete_downloads_after_played: false,
            agent_initial_model: "deepseek-v4-flash:cloud".to_owned(),
            agent_initial_model_name: "DeepSeek Flash".to_owned(),
            agent_thinking_model: "deepseek-v4-pro:cloud".to_owned(),
            agent_thinking_model_name: "DeepSeek Pro".to_owned(),
            memory_compilation_model: "deepseek-v4-flash:cloud".to_owned(),
            memory_compilation_model_name: "DeepSeek Flash".to_owned(),
            wiki_model: "deepseek-v4-flash:cloud".to_owned(),
            wiki_model_name: "DeepSeek Flash".to_owned(),
            categorization_model: "deepseek-v4-flash:cloud".to_owned(),
            categorization_model_name: "DeepSeek Flash".to_owned(),
            chapter_compilation_model: "deepseek-v4-flash:cloud".to_owned(),
            chapter_compilation_model_name: "DeepSeek Flash".to_owned(),
            embeddings_model: "deepseek-v4-flash:cloud".to_owned(),
            embeddings_model_name: "DeepSeek Flash".to_owned(),
            image_generation_model: "google/gemini-2.5-flash-image".to_owned(),
            image_generation_model_name: "Gemini 2.5 Flash".to_owned(),
            reranker_enabled: false,
            open_router_credential_source: String::new(),
            open_router_byok_key_id: None,
            open_router_byok_key_label: None,
            open_router_connected_at: None,
            ollama_credential_source: String::new(),
            ollama_byok_key_id: None,
            ollama_byok_key_label: None,
            ollama_connected_at: None,
            ollama_chat_url: String::new(),
            eleven_labs_credential_source: String::new(),
            eleven_labs_byok_key_id: None,
            eleven_labs_byok_key_label: None,
            eleven_labs_connected_at: None,
            stt_provider: "apple_native".to_owned(),
            open_router_whisper_model: "openai/whisper-1".to_owned(),
            assembly_ai_stt_model: "universal-3-pro,universal-2".to_owned(),
            eleven_labs_stt_model: "scribe_v1".to_owned(),
            eleven_labs_tts_model: "eleven_turbo_v2_5".to_owned(),
            eleven_labs_voice_id: String::new(),
            eleven_labs_voice_name: String::new(),
            blossom_server_url: "https://blossom.primal.net".to_owned(),
            youtube_extractor_url: None,
            wiki_auto_generate_on_transcript_ingest: false,
            auto_ingest_publisher_transcripts: true,
            auto_fallback_to_scribe: true,
            notify_on_new_episodes: true,
            notify_on_briefing_ready: true,
            nostr_enabled: false,
            nostr_relay_url: String::new(),
            nostr_public_relays: Vec::new(),
            nostr_profile_name: String::new(),
            nostr_profile_about: String::new(),
            nostr_profile_picture: String::new(),
            nostr_public_key_hex: None,
        }
    }
}

impl SettingsSnapshot {
    /// Returns true when the snapshot equals `Default::default()`. Used as
    /// the `skip_serializing_if` guard on
    /// [`super::snapshot::PodcastUpdate::settings`] so the empty-state
    /// snapshot stays byte-identical to the legacy stub (D6).
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}
