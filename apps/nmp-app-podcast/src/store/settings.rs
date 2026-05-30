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
}

#[cfg(test)]
#[path = "settings_tests.rs"]
mod tests;
