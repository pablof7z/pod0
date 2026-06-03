//! Settings accessors for [`super::PodcastStore`].
//!
//! Covers the onboarding-complete flag and the per-podcast auto-download
//! opt-in. Extracted to keep `store/mod.rs` within the 500-line ceiling.
//!
//! Persistence is handled by the parent module's `persist()` helper —
//! every mutator here calls `self.persist()` so changes survive restart.

use podcast_core::PodcastId;

use super::PodcastStore;

impl PodcastStore {
    /// Whether the user has finished the iOS onboarding flow. Read by the iOS
    /// shell from the `settings` snapshot to gate `OnboardingView`. Defaults
    /// to `false` for fresh installs.
    pub fn has_completed_onboarding(&self) -> bool {
        self.has_completed_onboarding
    }

    /// Update the onboarding-complete flag and flush to disk when a data dir
    /// is registered. Idempotent: writing the same value is a no-op for the
    /// disk file (the bytes are unchanged) and for the in-memory flag.
    pub fn set_onboarding_complete(&mut self, value: bool) {
        if self.has_completed_onboarding == value {
            return;
        }
        self.has_completed_onboarding = value;
        self.persist();
    }

    /// Set the auto-download opt-in flag for a podcast. Idempotent and
    /// silent when the podcast isn't subscribed (the flag will just
    /// hang around in the set; `unsubscribe` clears it). Flushes to
    /// disk when a data dir is bound so the preference survives
    /// app relaunches.
    pub fn set_auto_download(&mut self, podcast_id: PodcastId, enabled: bool) {
        let changed = if enabled {
            self.auto_download_enabled.insert(podcast_id)
        } else {
            self.auto_download_enabled.remove(&podcast_id)
        };
        if changed {
            self.persist();
        }
    }

    /// Read the auto-download opt-in flag for a podcast. Defaults to
    /// `false` for unknown / never-toggled podcasts.
    pub fn is_auto_download_enabled(&self, podcast_id: PodcastId) -> bool {
        self.auto_download_enabled.contains(&podcast_id)
    }

    /// Look up the auto-download flag by the string form of a podcast id.
    /// Helper for the FFI action handlers, which receive UUIDs as strings.
    pub fn is_auto_download_enabled_str(&self, id_str: &str) -> bool {
        match id_str.parse::<uuid::Uuid>() {
            Ok(uuid) => self.is_auto_download_enabled(PodcastId::new(uuid)),
            Err(_) => false,
        }
    }

    /// Whether to auto-advance to the next queued episode on `ItemEnd`.
    /// Default `true`. Controlled via `podcast.settings.set_auto_play_next`.
    pub fn auto_play_next(&self) -> bool {
        self.auto_play_next
    }

    /// Set the auto-play-next toggle and persist. Idempotent.
    pub fn set_auto_play_next(&mut self, value: bool) {
        if self.auto_play_next == value { return; }
        self.auto_play_next = value;
        self.persist();
    }

    /// Whether to mark the episode listened on natural `ItemEnd`.
    /// Default `true`.
    pub fn auto_mark_played_at_end(&self) -> bool {
        self.auto_mark_played_at_end
    }

    /// Set the auto-mark-played toggle and persist. Idempotent.
    pub fn set_auto_mark_played_at_end(&mut self, value: bool) {
        if self.auto_mark_played_at_end == value { return; }
        self.auto_mark_played_at_end = value;
        self.persist();
    }

    /// Raw action string for headphone double-tap gesture. Default `"skip_forward"`.
    pub fn headphone_double_tap_action(&self) -> &str {
        &self.headphone_double_tap_action
    }

    /// Raw action string for headphone triple-tap gesture. Default `"clip_now"`.
    pub fn headphone_triple_tap_action(&self) -> &str {
        &self.headphone_triple_tap_action
    }

    /// Update both headphone gesture action strings and persist. Idempotent.
    pub fn set_headphone_gesture_actions(&mut self, double_tap: String, triple_tap: String) {
        if self.headphone_double_tap_action == double_tap
            && self.headphone_triple_tap_action == triple_tap
        {
            return;
        }
        self.headphone_double_tap_action = double_tap;
        self.headphone_triple_tap_action = triple_tap;
        self.persist();
    }

    /// Skip-forward interval in seconds. Default 30.0; user-configurable via
    /// `podcast.settings.set_skip_intervals`.
    pub fn skip_forward_secs(&self) -> f64 {
        self.skip_forward_secs
    }

    /// Skip-backward interval in seconds. Default 15.0; user-configurable via
    /// `podcast.settings.set_skip_intervals`.
    pub fn skip_backward_secs(&self) -> f64 {
        self.skip_backward_secs
    }

    /// Set whether cellular auto-download is allowed for a podcast.
    /// `wifi_only=true` (the default) restricts auto-download to Wi-Fi.
    /// `wifi_only=false` allows downloads on any interface including cellular.
    pub fn set_wifi_only(&mut self, podcast_id: PodcastId, wifi_only: bool) {
        // `auto_download_cellular_allowed` tracks the explicit cellular-ok overrides.
        // Present = cellular allowed (wifi_only=false). Absent = wifi-only (default).
        let changed = if !wifi_only {
            self.auto_download_cellular_allowed.insert(podcast_id)
        } else {
            self.auto_download_cellular_allowed.remove(&podcast_id)
        };
        if changed {
            self.persist();
        }
    }

    /// Whether auto-download is Wi-Fi-gated for this podcast.
    /// Returns `true` (Wi-Fi-only) by default; `false` only when the user
    /// explicitly allowed cellular downloads via `set_wifi_only(false)`.
    pub fn wifi_only_for(&self, podcast_id: PodcastId) -> bool {
        !self.auto_download_cellular_allowed.contains(&podcast_id)
    }

    /// Whether the device's active network interface is Wi-Fi. Updated by
    /// `nmp.network.capability` `ConnectivityChanged` reports. Defaults to
    /// `true` (conservative: assume Wi-Fi until the capability fires).
    pub fn is_on_wifi(&self) -> bool {
        self.is_on_wifi
    }

    /// Update the Wi-Fi state from a `NetworkReport::ConnectivityChanged`
    /// event. Not persisted — this is a runtime signal, not durable config.
    pub fn set_is_on_wifi(&mut self, value: bool) {
        self.is_on_wifi = value;
    }

    /// Append deferred downloads (episodes that need Wi-Fi but the device
    /// was on cellular at refresh time).
    pub fn add_pending_wifi_downloads(&mut self, items: Vec<(String, String)>) {
        self.pending_wifi_downloads.extend(items);
    }

    /// Drain and return all pending Wi-Fi downloads. Called when Wi-Fi is
    /// restored so they can be dispatched immediately.
    pub fn drain_pending_wifi_downloads(&mut self) -> Vec<(String, String)> {
        std::mem::take(&mut self.pending_wifi_downloads)
    }

    /// Default playback rate. `1.0` is normal speed; clamped to `[0.5, 3.0]`.
    /// Persisted so the preferred rate survives restart.
    pub fn default_playback_rate(&self) -> f64 {
        self.default_playback_rate
    }

    /// Set the default playback rate and persist. Clamped to `[0.5, 3.0]`.
    pub fn set_default_playback_rate(&mut self, rate: f64) {
        let clamped = rate.clamp(0.5, 3.0);
        if (self.default_playback_rate - clamped).abs() < f64::EPSILON { return; }
        self.default_playback_rate = clamped;
        self.persist();
    }

    /// When `true`, downloaded files are deleted after the episode is marked played.
    pub fn auto_delete_downloads_after_played(&self) -> bool {
        self.auto_delete_downloads_after_played
    }

    /// Set the auto-delete-after-played toggle and persist. Idempotent.
    pub fn set_auto_delete_downloads_after_played(&mut self, value: bool) {
        if self.auto_delete_downloads_after_played == value { return; }
        self.auto_delete_downloads_after_played = value;
        self.persist();
    }

    /// Update both skip intervals. Clamps each value to `[1.0, 120.0]`
    /// seconds and persists when either value changes.
    pub fn set_skip_intervals(&mut self, forward_secs: f64, backward_secs: f64) {
        let fwd = forward_secs.clamp(1.0, 120.0);
        let bwd = backward_secs.clamp(1.0, 120.0);
        if (self.skip_forward_secs - fwd).abs() < f64::EPSILON
            && (self.skip_backward_secs - bwd).abs() < f64::EPSILON
        {
            return;
        }
        self.skip_forward_secs = fwd;
        self.skip_backward_secs = bwd;
        self.persist();
    }

    /// LLM model ID for initial agent chat. Default "deepseek-v4-flash:cloud".
    pub fn agent_initial_model(&self) -> &str {
        &self.agent_initial_model
    }

    /// Human-readable name for the initial agent model. Default "DeepSeek Flash".
    pub fn agent_initial_model_name(&self) -> &str {
        &self.agent_initial_model_name
    }

    /// Set both the model ID and name for initial agent chat. Idempotent.
    pub fn set_agent_initial_model(&mut self, model: String, model_name: String) {
        if self.agent_initial_model == model && self.agent_initial_model_name == model_name {
            return;
        }
        self.agent_initial_model = model;
        self.agent_initial_model_name = model_name;
        self.persist();
    }

    /// LLM model ID for agent thinking/planning. Default "deepseek-v4-pro:cloud".
    pub fn agent_thinking_model(&self) -> &str {
        &self.agent_thinking_model
    }

    /// Human-readable name for the agent thinking model. Default "DeepSeek Pro".
    pub fn agent_thinking_model_name(&self) -> &str {
        &self.agent_thinking_model_name
    }

    /// Set both the model ID and name for agent thinking/planning. Idempotent.
    pub fn set_agent_thinking_model(&mut self, model: String, model_name: String) {
        if self.agent_thinking_model == model && self.agent_thinking_model_name == model_name {
            return;
        }
        self.agent_thinking_model = model;
        self.agent_thinking_model_name = model_name;
        self.persist();
    }

    /// LLM model ID for memory compilation. Default "deepseek-v4-flash:cloud".
    pub fn memory_compilation_model(&self) -> &str {
        &self.memory_compilation_model
    }

    /// Human-readable name for the memory compilation model. Default "DeepSeek Flash".
    pub fn memory_compilation_model_name(&self) -> &str {
        &self.memory_compilation_model_name
    }

    /// Set both the model ID and name for memory compilation. Idempotent.
    pub fn set_memory_compilation_model(&mut self, model: String, model_name: String) {
        if self.memory_compilation_model == model && self.memory_compilation_model_name == model_name {
            return;
        }
        self.memory_compilation_model = model;
        self.memory_compilation_model_name = model_name;
        self.persist();
    }

    /// LLM model ID for wiki synthesis. Default "deepseek-v4-flash:cloud".
    pub fn wiki_model(&self) -> &str {
        &self.wiki_model
    }

    /// Human-readable name for the wiki model. Default "DeepSeek Flash".
    pub fn wiki_model_name(&self) -> &str {
        &self.wiki_model_name
    }

    /// Set both the model ID and name for wiki synthesis. Idempotent.
    pub fn set_wiki_model(&mut self, model: String, model_name: String) {
        if self.wiki_model == model && self.wiki_model_name == model_name {
            return;
        }
        self.wiki_model = model;
        self.wiki_model_name = model_name;
        self.persist();
    }

    /// LLM model ID for episode categorization. Default "deepseek-v4-flash:cloud".
    pub fn categorization_model(&self) -> &str {
        &self.categorization_model
    }

    /// Human-readable name for the categorization model. Default "DeepSeek Flash".
    pub fn categorization_model_name(&self) -> &str {
        &self.categorization_model_name
    }

    /// Set both the model ID and name for categorization. Idempotent.
    pub fn set_categorization_model(&mut self, model: String, model_name: String) {
        if self.categorization_model == model && self.categorization_model_name == model_name {
            return;
        }
        self.categorization_model = model;
        self.categorization_model_name = model_name;
        self.persist();
    }

    /// LLM model ID for chapter compilation. Default "deepseek-v4-flash:cloud".
    pub fn chapter_compilation_model(&self) -> &str {
        &self.chapter_compilation_model
    }

    /// Human-readable name for the chapter compilation model. Default "DeepSeek Flash".
    pub fn chapter_compilation_model_name(&self) -> &str {
        &self.chapter_compilation_model_name
    }

    /// Set both the model ID and name for chapter compilation. Idempotent.
    pub fn set_chapter_compilation_model(&mut self, model: String, model_name: String) {
        if self.chapter_compilation_model == model && self.chapter_compilation_model_name == model_name {
            return;
        }
        self.chapter_compilation_model = model;
        self.chapter_compilation_model_name = model_name;
        self.persist();
    }

    /// LLM model ID for embeddings generation. Default "deepseek-v4-flash:cloud".
    pub fn embeddings_model(&self) -> &str {
        &self.embeddings_model
    }

    /// Human-readable name for the embeddings model. Default "DeepSeek Flash".
    pub fn embeddings_model_name(&self) -> &str {
        &self.embeddings_model_name
    }

    /// Set both the model ID and name for embeddings. Idempotent.
    pub fn set_embeddings_model(&mut self, model: String, model_name: String) {
        if self.embeddings_model == model && self.embeddings_model_name == model_name {
            return;
        }
        self.embeddings_model = model;
        self.embeddings_model_name = model_name;
        self.persist();
    }

    /// LLM model ID for image generation. Default "google/gemini-2.5-flash-image".
    pub fn image_generation_model(&self) -> &str {
        &self.image_generation_model
    }

    /// Human-readable name for the image generation model. Default "Gemini 2.5 Flash".
    pub fn image_generation_model_name(&self) -> &str {
        &self.image_generation_model_name
    }

    /// Set both the model ID and name for image generation. Idempotent.
    pub fn set_image_generation_model(&mut self, model: String, model_name: String) {
        if self.image_generation_model == model && self.image_generation_model_name == model_name {
            return;
        }
        self.image_generation_model = model;
        self.image_generation_model_name = model_name;
        self.persist();
    }

    /// Whether the reranker is enabled for search results. Default `false`.
    pub fn reranker_enabled(&self) -> bool {
        self.reranker_enabled
    }

    /// Set the reranker-enabled toggle and persist. Idempotent.
    pub fn set_reranker_enabled(&mut self, value: bool) {
        if self.reranker_enabled == value { return; }
        self.reranker_enabled = value;
        self.persist();
    }

    /// OpenRouter credential source enum (as raw String: "apiKey", "byok", "nostr").
    pub fn open_router_credential_source(&self) -> &str {
        &self.open_router_credential_source
    }

    /// OpenRouter BYOK key ID (optional).
    pub fn open_router_byok_key_id(&self) -> Option<&str> {
        self.open_router_byok_key_id.as_deref()
    }

    /// OpenRouter BYOK key label (optional).
    pub fn open_router_byok_key_label(&self) -> Option<&str> {
        self.open_router_byok_key_label.as_deref()
    }

    /// OpenRouter credential connected-at timestamp (epoch seconds, optional).
    pub fn open_router_connected_at(&self) -> Option<i64> {
        self.open_router_connected_at
    }

    /// Set OpenRouter credential metadata. Coalesces source+key_id+key_label+connected_at
    /// into a single mutation so the guard fires once when any value changes.
    pub fn set_open_router_credential(
        &mut self,
        source: String,
        key_id: Option<String>,
        key_label: Option<String>,
        connected_at: Option<i64>,
    ) {
        if self.open_router_credential_source == source
            && self.open_router_byok_key_id == key_id
            && self.open_router_byok_key_label == key_label
            && self.open_router_connected_at == connected_at
        {
            return;
        }
        self.open_router_credential_source = source;
        self.open_router_byok_key_id = key_id;
        self.open_router_byok_key_label = key_label;
        self.open_router_connected_at = connected_at;
        self.persist();
    }

    /// Ollama credential source enum (as raw String: "apiKey", "byok", "nostr").
    pub fn ollama_credential_source(&self) -> &str {
        &self.ollama_credential_source
    }

    /// Ollama BYOK key ID (optional).
    pub fn ollama_byok_key_id(&self) -> Option<&str> {
        self.ollama_byok_key_id.as_deref()
    }

    /// Ollama BYOK key label (optional).
    pub fn ollama_byok_key_label(&self) -> Option<&str> {
        self.ollama_byok_key_label.as_deref()
    }

    /// Ollama credential connected-at timestamp (epoch seconds, optional).
    pub fn ollama_connected_at(&self) -> Option<i64> {
        self.ollama_connected_at
    }

    /// Set Ollama credential metadata. Coalesces source+key_id+key_label+connected_at
    /// into a single mutation so the guard fires once when any value changes.
    pub fn set_ollama_credential(
        &mut self,
        source: String,
        key_id: Option<String>,
        key_label: Option<String>,
        connected_at: Option<i64>,
    ) {
        if self.ollama_credential_source == source
            && self.ollama_byok_key_id == key_id
            && self.ollama_byok_key_label == key_label
            && self.ollama_connected_at == connected_at
        {
            return;
        }
        self.ollama_credential_source = source;
        self.ollama_byok_key_id = key_id;
        self.ollama_byok_key_label = key_label;
        self.ollama_connected_at = connected_at;
        self.persist();
    }

    /// Ollama chat endpoint URL for LLM inference.
    pub fn ollama_chat_url(&self) -> &str {
        &self.ollama_chat_url
    }

    /// Set Ollama chat URL and persist. Idempotent.
    pub fn set_ollama_chat_url(&mut self, url: String) {
        if self.ollama_chat_url == url { return; }
        self.ollama_chat_url = url;
        self.persist();
    }

    /// ElevenLabs credential source enum (as raw String: "apiKey", "byok", "nostr").
    pub fn eleven_labs_credential_source(&self) -> &str {
        &self.eleven_labs_credential_source
    }

    /// ElevenLabs BYOK key ID (optional).
    pub fn eleven_labs_byok_key_id(&self) -> Option<&str> {
        self.eleven_labs_byok_key_id.as_deref()
    }

    /// ElevenLabs BYOK key label (optional).
    pub fn eleven_labs_byok_key_label(&self) -> Option<&str> {
        self.eleven_labs_byok_key_label.as_deref()
    }

    /// ElevenLabs credential connected-at timestamp (epoch seconds, optional).
    pub fn eleven_labs_connected_at(&self) -> Option<i64> {
        self.eleven_labs_connected_at
    }

    /// Set ElevenLabs credential metadata. Coalesces source+key_id+key_label+connected_at
    /// into a single mutation so the guard fires once when any value changes.
    pub fn set_eleven_labs_credential(
        &mut self,
        source: String,
        key_id: Option<String>,
        key_label: Option<String>,
        connected_at: Option<i64>,
    ) {
        if self.eleven_labs_credential_source == source
            && self.eleven_labs_byok_key_id == key_id
            && self.eleven_labs_byok_key_label == key_label
            && self.eleven_labs_connected_at == connected_at
        {
            return;
        }
        self.eleven_labs_credential_source = source;
        self.eleven_labs_byok_key_id = key_id;
        self.eleven_labs_byok_key_label = key_label;
        self.eleven_labs_connected_at = connected_at;
        self.persist();
    }

    /// STT provider selection (enum .rawValue String).
    /// Default `"apple_native"`.
    pub fn stt_provider(&self) -> &str {
        &self.stt_provider
    }

    /// Set the STT provider and persist. Idempotent.
    pub fn set_stt_provider(&mut self, value: String) {
        if self.stt_provider == value { return; }
        self.stt_provider = value;
        self.persist();
    }

    /// OpenRouter Whisper model string. Default `"openai/whisper-1"`.
    pub fn open_router_whisper_model(&self) -> &str {
        &self.open_router_whisper_model
    }

    /// Set the OpenRouter Whisper model and persist. Idempotent.
    pub fn set_open_router_whisper_model(&mut self, value: String) {
        if self.open_router_whisper_model == value { return; }
        self.open_router_whisper_model = value;
        self.persist();
    }

    /// AssemblyAI STT model string. Default `"universal-3-pro,universal-2"`.
    pub fn assembly_ai_stt_model(&self) -> &str {
        &self.assembly_ai_stt_model
    }

    /// Set the AssemblyAI STT model and persist. Idempotent.
    pub fn set_assembly_ai_stt_model(&mut self, value: String) {
        if self.assembly_ai_stt_model == value { return; }
        self.assembly_ai_stt_model = value;
        self.persist();
    }

    /// ElevenLabs STT model string. Default `"scribe_v1"`.
    pub fn eleven_labs_stt_model(&self) -> &str {
        &self.eleven_labs_stt_model
    }

    /// ElevenLabs TTS model string. Default `"eleven_turbo_v2_5"`.
    pub fn eleven_labs_tts_model(&self) -> &str {
        &self.eleven_labs_tts_model
    }

    /// Set both ElevenLabs STT and TTS models and persist. Atomic update. Idempotent.
    pub fn set_eleven_labs_models(&mut self, stt_model: String, tts_model: String) {
        if self.eleven_labs_stt_model == stt_model && self.eleven_labs_tts_model == tts_model {
            return;
        }
        self.eleven_labs_stt_model = stt_model;
        self.eleven_labs_tts_model = tts_model;
        self.persist();
    }

    /// ElevenLabs voice ID. Defaults to empty string.
    pub fn eleven_labs_voice_id(&self) -> &str {
        &self.eleven_labs_voice_id
    }

    /// ElevenLabs voice name. Defaults to empty string.
    pub fn eleven_labs_voice_name(&self) -> &str {
        &self.eleven_labs_voice_name
    }

    /// Set both ElevenLabs voice ID and name and persist. Atomic update. Idempotent.
    pub fn set_eleven_labs_voice(&mut self, voice_id: String, voice_name: String) {
        if self.eleven_labs_voice_id == voice_id && self.eleven_labs_voice_name == voice_name {
            return;
        }
        self.eleven_labs_voice_id = voice_id;
        self.eleven_labs_voice_name = voice_name;
        self.persist();
    }

    /// Blossom server URL. Default `"https://blossom.primal.net"`.
    pub fn blossom_server_url(&self) -> &str {
        &self.blossom_server_url
    }

    /// Set the Blossom server URL and persist. Idempotent.
    pub fn set_blossom_server_url(&mut self, value: String) {
        if self.blossom_server_url == value { return; }
        self.blossom_server_url = value;
        self.persist();
    }

    /// YouTube extractor URL (optional).
    pub fn youtube_extractor_url(&self) -> Option<&str> {
        self.youtube_extractor_url.as_deref()
    }

    /// Set the YouTube extractor URL and persist. Idempotent.
    pub fn set_youtube_extractor_url(&mut self, value: Option<String>) {
        if self.youtube_extractor_url == value { return; }
        self.youtube_extractor_url = value;
        self.persist();
    }

    /// Whether to auto-generate wiki entries when transcripts are ingested. Default `false`.
    pub fn wiki_auto_generate_on_transcript_ingest(&self) -> bool {
        self.wiki_auto_generate_on_transcript_ingest
    }

    /// Set the wiki-auto-generate-on-transcript-ingest toggle and persist. Idempotent.
    pub fn set_wiki_auto_generate_on_transcript_ingest(&mut self, value: bool) {
        if self.wiki_auto_generate_on_transcript_ingest == value { return; }
        self.wiki_auto_generate_on_transcript_ingest = value;
        self.persist();
    }

    /// Whether to auto-ingest publisher-provided transcripts. Default `true`.
    pub fn auto_ingest_publisher_transcripts(&self) -> bool {
        self.auto_ingest_publisher_transcripts
    }

    /// Set the auto-ingest-publisher-transcripts toggle and persist. Idempotent.
    pub fn set_auto_ingest_publisher_transcripts(&mut self, value: bool) {
        if self.auto_ingest_publisher_transcripts == value { return; }
        self.auto_ingest_publisher_transcripts = value;
        self.persist();
    }

    /// Whether to fall back to Scribe (STT) when publisher transcript ingestion fails. Default `true`.
    pub fn auto_fallback_to_scribe(&self) -> bool {
        self.auto_fallback_to_scribe
    }

    /// Set the auto-fallback-to-scribe toggle and persist. Idempotent.
    pub fn set_auto_fallback_to_scribe(&mut self, value: bool) {
        if self.auto_fallback_to_scribe == value { return; }
        self.auto_fallback_to_scribe = value;
        self.persist();
    }

    /// Whether to send local notifications when new episodes arrive. Default `true`.
    pub fn notify_on_new_episodes(&self) -> bool {
        self.notify_on_new_episodes
    }

    /// Set the notify-on-new-episodes toggle and persist. Idempotent.
    pub fn set_notify_on_new_episodes(&mut self, value: bool) {
        if self.notify_on_new_episodes == value { return; }
        self.notify_on_new_episodes = value;
        self.persist();
    }

    /// Whether Nostr publishing and identity features are enabled. Default `false`.
    pub fn nostr_enabled(&self) -> bool {
        self.nostr_enabled
    }

    /// Set the nostr-enabled toggle and persist. Idempotent.
    pub fn set_nostr_enabled(&mut self, value: bool) {
        if self.nostr_enabled == value { return; }
        self.nostr_enabled = value;
        self.persist();
    }

    /// Primary Nostr relay URL for publishing and event distribution.
    pub fn nostr_relay_url(&self) -> &str {
        &self.nostr_relay_url
    }

    /// Set the Nostr relay URL and persist. Idempotent.
    pub fn set_nostr_relay_url(&mut self, url: String) {
        if self.nostr_relay_url == url { return; }
        self.nostr_relay_url = url;
        self.persist();
    }

    /// List of public Nostr relay URLs for broadcast and subscription.
    pub fn nostr_public_relays(&self) -> &[String] {
        &self.nostr_public_relays
    }

    /// Set the list of public Nostr relays and persist. Idempotent.
    pub fn set_nostr_public_relays(&mut self, relays: Vec<String>) {
        if self.nostr_public_relays == relays { return; }
        self.nostr_public_relays = relays;
        self.persist();
    }

    /// User's display name in Nostr profile metadata.
    pub fn nostr_profile_name(&self) -> &str {
        &self.nostr_profile_name
    }

    /// User's about/bio text in Nostr profile metadata.
    pub fn nostr_profile_about(&self) -> &str {
        &self.nostr_profile_about
    }

    /// User's picture URL in Nostr profile metadata.
    pub fn nostr_profile_picture(&self) -> &str {
        &self.nostr_profile_picture
    }

    /// Set all three profile fields (name, about, picture) atomically and persist. Idempotent.
    pub fn set_nostr_profile(&mut self, name: String, about: String, picture: String) {
        if self.nostr_profile_name == name
            && self.nostr_profile_about == about
            && self.nostr_profile_picture == picture
        {
            return;
        }
        self.nostr_profile_name = name;
        self.nostr_profile_about = about;
        self.nostr_profile_picture = picture;
        self.persist();
    }

    /// Nostr public key hex (read-only, derived from Keychain). Not persisted.
    pub fn nostr_public_key_hex(&self) -> Option<&str> {
        self.nostr_public_key_hex.as_deref()
    }

    /// Set the Nostr public key hex. Not persisted; used only for snapshot projection.
    pub fn set_nostr_public_key_hex(&mut self, hex: Option<String>) {
        self.nostr_public_key_hex = hex;
    }
}

#[cfg(test)]
#[path = "settings_tests.rs"]
mod tests;
