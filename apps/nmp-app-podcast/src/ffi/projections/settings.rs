use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

/// App-settings projection surfaced via
/// [`super::snapshot::PodcastUpdate::settings`].
///
/// Replaces the legacy in-memory `Settings` compat shim. The kernel
/// authoritative source is [`crate::store::PodcastStore`] accessors.
///
/// Carries `#[serde(default)]` at the container level so a wire payload that
/// omits any field (or the whole `settings` object) decodes to the
/// fresh-install state. That fresh-install state is **not** a literal here:
/// [`Default`] builds it by projecting [`crate::store::PodcastStore::new`] —
/// the single canonical defaults site — through the snapshot builder, so the
/// Rust and Swift mirrors cannot drift (enforced by the
/// `settings_fresh_install.json` fixture test).
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(default)]
pub struct SettingsSnapshot {
    /// Whether the user has finished the iOS onboarding flow.
    pub has_completed_onboarding: bool,
    /// When `true`, the player actor seeks past each ad segment.
    pub auto_skip_ads_enabled: bool,
    /// When `true`, the kernel auto-advances to the next queued episode
    /// on `ItemEnd`.
    pub auto_play_next: bool,
    /// When `true`, the kernel marks the episode listened on `ItemEnd`.
    pub auto_mark_played_at_end: bool,
    /// Raw action string for headphone double-tap gesture.
    pub headphone_double_tap_action: String,
    /// Raw action string for headphone triple-tap gesture.
    pub headphone_triple_tap_action: String,
    /// Skip-forward interval in seconds.
    pub skip_forward_secs: f64,
    /// Skip-backward interval in seconds.
    pub skip_backward_secs: f64,
    /// Default playback rate; range [0.5, 3.0].
    pub default_playback_rate: f64,
    /// When `true`, downloaded files are deleted after the episode is marked played.
    pub auto_delete_downloads_after_played: bool,
    /// LLM model ID for initial agent chat.
    pub agent_initial_model: String,
    /// Human-readable name for initial agent model.
    pub agent_initial_model_name: String,
    /// LLM model ID for agent thinking/planning.
    pub agent_thinking_model: String,
    /// Human-readable name for agent thinking model.
    pub agent_thinking_model_name: String,
    /// LLM model ID for memory compilation.
    pub memory_compilation_model: String,
    /// Human-readable name for memory compilation model.
    pub memory_compilation_model_name: String,
    /// LLM model ID for episode categorization.
    pub categorization_model: String,
    /// Human-readable name for categorization model.
    pub categorization_model_name: String,
    /// LLM model ID for chapter compilation.
    pub chapter_compilation_model: String,
    /// Human-readable name for chapter compilation model.
    pub chapter_compilation_model_name: String,
    /// LLM model ID for embeddings generation.
    pub embeddings_model: String,
    /// Human-readable name for embeddings model.
    pub embeddings_model_name: String,
    /// LLM model ID for image generation.
    pub image_generation_model: String,
    /// Human-readable name for image generation model.
    pub image_generation_model_name: String,
    /// Whether the reranker is enabled for search results.
    pub reranker_enabled: bool,
    /// OpenRouter credential source enum (raw String: "apiKey", "byok", "nostr").
    pub open_router_credential_source: String,
    /// Whether an OpenRouter API key is currently loaded in the in-memory
    /// provider cache. The secret never crosses FFI.
    pub open_router_key_present: bool,
    /// OpenRouter BYOK key ID (optional).
    pub open_router_byok_key_id: Option<String>,
    /// OpenRouter BYOK key label (optional).
    pub open_router_byok_key_label: Option<String>,
    /// OpenRouter credential connected-at timestamp (epoch seconds, optional).
    pub open_router_connected_at: Option<i64>,
    /// Ollama credential source enum (raw String: "apiKey", "byok", "nostr").
    pub ollama_credential_source: String,
    /// Whether an Ollama API key is currently loaded in the in-memory provider
    /// cache. The secret never crosses FFI.
    pub ollama_key_present: bool,
    /// Ollama BYOK key ID (optional).
    pub ollama_byok_key_id: Option<String>,
    /// Ollama BYOK key label (optional).
    pub ollama_byok_key_label: Option<String>,
    /// Ollama credential connected-at timestamp (epoch seconds, optional).
    pub ollama_connected_at: Option<i64>,
    /// Ollama chat endpoint URL for LLM inference.
    pub ollama_chat_url: String,
    /// ElevenLabs credential source enum (raw String: "apiKey", "byok", "nostr").
    pub eleven_labs_credential_source: String,
    /// Whether an ElevenLabs API key is currently loaded in the in-memory
    /// provider cache. The secret never crosses FFI.
    pub eleven_labs_key_present: bool,
    /// ElevenLabs BYOK key ID (optional).
    pub eleven_labs_byok_key_id: Option<String>,
    /// ElevenLabs BYOK key label (optional).
    pub eleven_labs_byok_key_label: Option<String>,
    /// ElevenLabs credential connected-at timestamp (epoch seconds, optional).
    pub eleven_labs_connected_at: Option<i64>,
    /// AssemblyAI credential source enum (raw String: "manual", "byok", "none").
    pub assembly_ai_credential_source: String,
    /// AssemblyAI BYOK key ID (optional).
    pub assembly_ai_byok_key_id: Option<String>,
    /// AssemblyAI BYOK key label (optional).
    pub assembly_ai_byok_key_label: Option<String>,
    /// AssemblyAI credential connected-at timestamp (epoch seconds, optional).
    pub assembly_ai_connected_at: Option<i64>,
    /// Perplexity credential source enum (raw String: "manual", "byok", "none").
    pub perplexity_credential_source: String,
    /// Perplexity BYOK key ID (optional).
    pub perplexity_byok_key_id: Option<String>,
    /// Perplexity BYOK key label (optional).
    pub perplexity_byok_key_label: Option<String>,
    /// Perplexity credential connected-at timestamp (epoch seconds, optional).
    pub perplexity_connected_at: Option<i64>,
    /// STT provider selection enum (raw String: "apple_native", etc).
    pub stt_provider: String,
    /// Kernel-resolved effective STT provider after applying the fallback
    /// policy (raw String: "apple_native" | "elevenlabs_scribe" |
    /// "assemblyai" | "openrouter_whisper").
    ///
    /// This is what callers should act on, NOT `stt_provider`: it already
    /// accounts for a key-requiring provider whose key is absent (downgraded
    /// to "apple_native"). Computed in the snapshot builder from `stt_provider`
    /// + the present-key set Swift reports via
    /// `podcast.settings.set_stt_keys_present`.
    pub effective_stt_provider: String,
    /// Whether the *resolved* `effective_stt_provider` needs an API key to
    /// run. Always `false` when the policy fell back to "apple_native". A UI
    /// can read this to know whether transcription will hit the network.
    pub effective_stt_provider_requires_key: bool,
    /// Whether an AssemblyAI API key is currently loaded in the in-memory
    /// provider cache. The secret never crosses FFI.
    pub assembly_ai_key_present: bool,
    /// Whether a Perplexity API key is currently loaded in the in-memory
    /// provider cache. The secret never crosses FFI.
    pub perplexity_key_present: bool,
    /// OpenRouter Whisper model string.
    pub open_router_whisper_model: String,
    /// AssemblyAI STT model string.
    pub assembly_ai_stt_model: String,
    /// ElevenLabs STT model string.
    pub eleven_labs_stt_model: String,
    /// ElevenLabs TTS model string.
    pub eleven_labs_tts_model: String,
    /// ElevenLabs voice ID. Defaults to empty string.
    pub eleven_labs_voice_id: String,
    /// ElevenLabs voice name. Defaults to empty string.
    pub eleven_labs_voice_name: String,
    /// Blossom server URL.
    pub blossom_server_url: String,
    /// YouTube extractor URL (optional).
    pub youtube_extractor_url: Option<String>,
    /// Local on-device LLM model ID (optional).
    pub local_model_id: Option<String>,
    /// Whether to auto-ingest publisher-provided transcripts.
    pub auto_ingest_publisher_transcripts: bool,
    /// Whether to fall back to Scribe (STT) when publisher transcript ingestion fails.
    pub auto_fallback_to_scribe: bool,
    /// Whether to send local notifications when new episodes arrive.
    pub notify_on_new_episodes: bool,
    /// Whether Nostr publishing and identity features are enabled.
    pub nostr_enabled: bool,
    /// Primary Nostr relay URL for publishing and event distribution. Default empty.
    pub nostr_relay_url: String,
    /// User's display name in Nostr profile metadata. Default empty.
    pub nostr_profile_name: String,
    /// User's about/bio text in Nostr profile metadata. Default empty.
    pub nostr_profile_about: String,
    /// User's picture URL in Nostr profile metadata. Default empty.
    pub nostr_profile_picture: String,
    /// Nostr public key hex (read-only, derived from Keychain). Not persisted.
    pub nostr_public_key_hex: Option<String>,
}

/// Cached fresh-install snapshot. Building it spins up a throwaway
/// [`crate::store::PodcastStore`], so memoize it: `Default` is called on every
/// wire decode that omits the `settings` object.
static FRESH_INSTALL: OnceLock<SettingsSnapshot> = OnceLock::new();

impl Default for SettingsSnapshot {
    /// Project the single canonical defaults site
    /// ([`crate::store::PodcastStore::new`]) through the same builder the live
    /// snapshot path uses. No literal defaults live here.
    fn default() -> Self {
        FRESH_INSTALL
            .get_or_init(|| {
                let store = crate::store::PodcastStore::new();
                crate::ffi::snapshot_settings::build_settings_snapshot(&store)
            })
            .clone()
    }
}
