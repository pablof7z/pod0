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
        if self.auto_play_next == value {
            return;
        }
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
        if self.auto_mark_played_at_end == value {
            return;
        }
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
        if (self.default_playback_rate - clamped).abs() < f64::EPSILON {
            return;
        }
        self.default_playback_rate = clamped;
        self.persist();
    }

    /// When `true`, downloaded files are deleted after the episode is marked played.
    pub fn auto_delete_downloads_after_played(&self) -> bool {
        self.auto_delete_downloads_after_played
    }

    /// Set the auto-delete-after-played toggle and persist. Idempotent.
    pub fn set_auto_delete_downloads_after_played(&mut self, value: bool) {
        if self.auto_delete_downloads_after_played == value {
            return;
        }
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

    /// Blossom server URL. Default `"https://blossom.primal.net"`.
    pub fn blossom_server_url(&self) -> &str {
        &self.blossom_server_url
    }

    /// Set the Blossom server URL and persist. Idempotent.
    pub fn set_blossom_server_url(&mut self, value: String) {
        if self.blossom_server_url == value {
            return;
        }
        self.blossom_server_url = value;
        self.persist();
    }

    /// YouTube extractor URL (optional).
    pub fn youtube_extractor_url(&self) -> Option<&str> {
        self.youtube_extractor_url.as_deref()
    }

    /// Set the YouTube extractor URL and persist. Idempotent.
    pub fn set_youtube_extractor_url(&mut self, value: Option<String>) {
        if self.youtube_extractor_url == value {
            return;
        }
        self.youtube_extractor_url = value;
        self.persist();
    }

    /// Local on-device LLM model ID (optional). When set, this dominates all callers
    /// in the LLM factory.
    pub fn local_model_id(&self) -> Option<&str> {
        self.local_model_id.as_deref()
    }

    /// Set the local model ID and persist. Idempotent.
    pub fn set_local_model_id(&mut self, value: Option<String>) {
        if self.local_model_id == value {
            return;
        }
        self.local_model_id = value;
        self.persist();
    }

    /// Whether to auto-generate wiki entries when transcripts are ingested. Default `false`.
    pub fn wiki_auto_generate_on_transcript_ingest(&self) -> bool {
        self.wiki_auto_generate_on_transcript_ingest
    }

    /// Set the wiki-auto-generate-on-transcript-ingest toggle and persist. Idempotent.
    pub fn set_wiki_auto_generate_on_transcript_ingest(&mut self, value: bool) {
        if self.wiki_auto_generate_on_transcript_ingest == value {
            return;
        }
        self.wiki_auto_generate_on_transcript_ingest = value;
        self.persist();
    }

    /// Whether to auto-ingest publisher-provided transcripts. Default `true`.
    pub fn auto_ingest_publisher_transcripts(&self) -> bool {
        self.auto_ingest_publisher_transcripts
    }

    /// Set the auto-ingest-publisher-transcripts toggle and persist. Idempotent.
    pub fn set_auto_ingest_publisher_transcripts(&mut self, value: bool) {
        if self.auto_ingest_publisher_transcripts == value {
            return;
        }
        self.auto_ingest_publisher_transcripts = value;
        self.persist();
    }

    /// Whether to fall back to Scribe (STT) when publisher transcript ingestion fails. Default `true`.
    pub fn auto_fallback_to_scribe(&self) -> bool {
        self.auto_fallback_to_scribe
    }

    /// Set the auto-fallback-to-scribe toggle and persist. Idempotent.
    pub fn set_auto_fallback_to_scribe(&mut self, value: bool) {
        if self.auto_fallback_to_scribe == value {
            return;
        }
        self.auto_fallback_to_scribe = value;
        self.persist();
    }

    /// Whether to send local notifications when new episodes arrive. Default `true`.
    pub fn notify_on_new_episodes(&self) -> bool {
        self.notify_on_new_episodes
    }

    /// Set the notify-on-new-episodes toggle and persist. Idempotent.
    pub fn set_notify_on_new_episodes(&mut self, value: bool) {
        if self.notify_on_new_episodes == value {
            return;
        }
        self.notify_on_new_episodes = value;
        self.persist();
    }

    /// Whether Nostr publishing and identity features are enabled. Default `false`.
    pub fn nostr_enabled(&self) -> bool {
        self.nostr_enabled
    }

    /// Set the nostr-enabled toggle and persist. Idempotent.
    pub fn set_nostr_enabled(&mut self, value: bool) {
        if self.nostr_enabled == value {
            return;
        }
        self.nostr_enabled = value;
        self.persist();
    }

    /// Primary Nostr relay URL for publishing and event distribution.
    pub fn nostr_relay_url(&self) -> &str {
        &self.nostr_relay_url
    }

    /// Set the Nostr relay URL and persist. Idempotent.
    pub fn set_nostr_relay_url(&mut self, url: String) {
        if self.nostr_relay_url == url {
            return;
        }
        self.nostr_relay_url = url;
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

    /// OpenRouter API key (in-memory only; never persisted to disk).
    pub fn open_router_api_key(&self) -> Option<&str> {
        self.open_router_api_key.as_deref()
    }

    /// Ollama API key (in-memory only; never persisted to disk).
    pub fn ollama_api_key(&self) -> Option<&str> {
        self.ollama_api_key.as_deref()
    }

    /// ElevenLabs API key (in-memory only; never persisted to disk).
    pub fn eleven_labs_api_key(&self) -> Option<&str> {
        self.eleven_labs_api_key.as_deref()
    }

    /// AssemblyAI API key (in-memory only; never persisted to disk).
    pub fn assembly_ai_api_key(&self) -> Option<&str> {
        self.assembly_ai_api_key.as_deref()
    }

    /// Perplexity API key (in-memory only; never persisted to disk).
    pub fn perplexity_api_key(&self) -> Option<&str> {
        self.perplexity_api_key.as_deref()
    }

    /// Set provider API keys in-memory. Does NOT call `persist()`; these keys
    /// never touch disk. Idempotent.
    pub fn set_provider_api_keys(
        &mut self,
        open_router: Option<String>,
        ollama: Option<String>,
        eleven_labs: Option<String>,
        assembly_ai: Option<String>,
        perplexity: Option<String>,
    ) {
        self.open_router_api_key = open_router;
        self.ollama_api_key = ollama;
        self.eleven_labs_api_key = eleven_labs;
        self.assembly_ai_api_key = assembly_ai;
        self.perplexity_api_key = perplexity;
    }
}

#[cfg(test)]
#[path = "settings_tests.rs"]
mod tests;
