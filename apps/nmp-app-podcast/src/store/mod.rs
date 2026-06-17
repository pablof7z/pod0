//! Podcast library store.
//!
//! Holds known podcasts, their episodes, and explicit follow membership. Keyed
//! by `PodcastId` so lookups are O(1); the store is wrapped in
//! `Arc<Mutex<PodcastStore>>` and shared between snapshot readers and writers.
//!
//! ## Persistence
//!
//! When [`PodcastStore::set_data_dir`] has been called the store mirrors every
//! mutation (`subscribe` / `unsubscribe` / `update_refresh_metadata`) to a
//! single `podcasts.json` file inside that directory. Reads stay purely
//! in-memory; the disk file is a write-through cache so the library survives
//! app restarts.
//!
//! D6: persistence failures degrade silently — the in-memory store remains
//! authoritative and the next mutation will try to write again.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use podcast_core::{Episode, EpisodeId, Podcast, PodcastId};

use crate::llm::provider_config::DEFAULT_OLLAMA_CHAT_URL;
use crate::queue::QueuedPlaybackItem;

mod ad_segments;
pub mod agent_tasks;
pub mod auto_download;
mod chapters;
pub mod clip_records;
mod credential_metadata;
mod disk;
mod download_persistence;
pub mod events;
pub mod identity;
pub mod agent_note_responder_cache;
pub mod approved_peer_store;
pub mod inbox_triage_cache;
pub mod outbound_turn_cache;
mod library;
pub mod metadata_index_backfill;
mod memory;
pub(crate) mod owned_ext;
mod persistence;
mod playback;
pub mod podcast_keys;
mod provider_settings;
pub mod relay_config;
mod settings;
pub mod stt_policy;
pub(crate) mod summary;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_ext;
mod podcast_user_categories;
mod transcripts;
mod triage_state;

use crate::ffi::projections::MemoryFact;
use crate::player::AdSegment;
pub use auto_download::{episodes_to_auto_download, AutoDownloadMode};
pub use metadata_index_backfill::{METADATA_INDEX_BACKFILL_BATCH_SIZE, METADATA_INDEX_INTER_BATCH_DELAY_MS};
use credential_metadata::ProviderCredentialMetadata;
pub use podcast_keys::PodcastKeyStore;

/// Backing store for known podcasts, follow membership and episode lists.
///
/// Mutations flush to `data_dir/podcasts.json` (atomic temp+rename) when a
/// data dir has been registered via [`Self::set_data_dir`]. Without a data
/// dir the store stays in memory — useful for unit tests and the very first
/// run before iOS calls `nmp_app_podcast_set_data_dir`.
pub struct PodcastStore {
    pub(super) podcasts: HashMap<PodcastId, Podcast>,
    pub(super) episodes: HashMap<PodcastId, Vec<Episode>>,
    followed_podcasts: HashSet<PodcastId>,
    /// Per-episode on-disk path for downloaded enclosures. Populated when an
    /// iOS `DownloadCapability` reports `Completed`; cleared by
    /// [`PodcastStore::clear_local_path`] when the user deletes the file.
    ///
    /// Lives in a side-map so refreshing a feed, which replaces the episode
    /// list wholesale, does not wipe download state.
    local_paths: HashMap<EpisodeId, String>,
    /// Per-episode downloaded-file size in bytes, recorded alongside
    /// [`local_paths`] at download-completion time so the snapshot projection
    /// can surface `EpisodeSummary::file_size_bytes` without a per-tick
    /// `std::fs::metadata` syscall (the read path runs on the main thread).
    ///
    /// Lifecycle-locked to `local_paths`: every `set_local_path` records a
    /// size and every `clear_local_path` drops it, so the size is never
    /// staler than the path it describes.
    file_sizes: HashMap<EpisodeId, i64>,
    /// Plain-text transcripts keyed by the string form of `EpisodeId`.
    transcripts: HashMap<String, String>,
    /// Timed transcript entries keyed by the string form of `EpisodeId`.
    /// Populated when iOS sends the structured `transcript_report` payload
    /// with `"entries"` (slice 5a). Absent for legacy plain-text callers.
    /// Used by `index_episode` to produce RAG chunks with real
    /// `start_secs` / `end_secs` so transcript-search hits can seek to the
    /// right position. Session durability (not persisted; re-populated on
    /// each STT run).
    timed_transcripts: HashMap<String, Vec<podcast_transcripts::TranscriptEntry>>,
    /// Last position (seconds) committed to disk for each episode, keyed by
    /// the string form of `EpisodeId`. Used by the writeback layer to decide
    /// whether the live playhead has drifted enough from the on-disk
    /// checkpoint to warrant another `persist()`. Cleared on `set_data_dir`
    /// since a freshly-bound store hasn't flushed anything yet — the
    /// hydrated values from disk are themselves the most-recent checkpoint.
    /// Not persisted: this is a runtime throttling marker, not durable state.
    last_flushed_positions: HashMap<String, f64>,
    /// Whether the user has finished the iOS onboarding flow. Surfaced via
    /// the `settings` snapshot projection so the iOS shell can decide
    /// whether to present `OnboardingView`. Mirrored to disk under the same
    /// `podcasts.json` envelope as the library so the flag survives restart.
    has_completed_onboarding: bool,
    /// Podcasts the user has opted into auto-download for.
    ///
    /// Membership is the policy: present ⇒ `handle_refresh` will queue
    /// freshly-discovered episodes via the download capability; absent ⇒
    /// new episodes are surfaced in the snapshot but not downloaded.
    /// Cleared by `unsubscribe` so a later re-subscribe starts fresh.
    ///
    /// Kept for legacy `is_auto_download_enabled` queries and back-compat
    /// disk load (old files that lack `auto_download_mode`). When
    /// `auto_download_modes` has an entry for a podcast, that entry is
    /// authoritative; this set is derived from it (non-Off ⇒ present).
    auto_download_enabled: HashSet<PodcastId>,
    /// Per-podcast typed auto-download mode (D7). Supersedes the old bool.
    /// `Off` / `LatestN(n)` / `AllNew`. Absent ⇒ derived from
    /// `auto_download_enabled` on disk load (true→AllNew, false/absent→Off).
    /// Cleared by `unsubscribe`.
    auto_download_modes: HashMap<PodcastId, AutoDownloadMode>,
    /// Podcasts for which cellular auto-download is **explicitly allowed**
    /// (i.e. the user set Wi-Fi-only to `false`). Absence means the default
    /// applies: Wi-Fi-only (matching `AutoDownloadPolicy.default.wifiOnly`).
    /// Cleared by `unsubscribe`.
    auto_download_cellular_allowed: HashSet<PodcastId>,
    /// Per-podcast transcription disabled set. Stores the IDs of podcasts for
    /// which the user has explicitly disabled transcription. Absence means
    /// enabled (the default). Persisted in `podcasts.json` as
    /// `transcription_disabled`.
    /// Cleared by `unsubscribe` so a re-subscribe starts fresh.
    transcription_disabled: HashSet<PodcastId>,
    /// Per-podcast notification disabled set. Absence means new-episode
    /// notifications are allowed when the global notification setting is on.
    /// Cleared by `unsubscribe` so a re-subscribe starts fresh.
    notifications_disabled: HashSet<PodcastId>,
    /// Episodes deferred because the device was on cellular when the feed
    /// refreshed and the show is Wi-Fi-only. These are dispatched as a batch
    /// the next time `NetworkReport::ConnectivityChanged { is_wifi: true }`
    /// arrives. Keyed by `(episode_id_str, enclosure_url)`.
    /// Not persisted — a cold launch on Wi-Fi will re-discover them naturally
    /// via the next feed refresh; deferred entries represent at most the
    /// downloads that were missed in the current session.
    pub(super) pending_wifi_downloads: Vec<(String, String)>,
    /// Durable agent-memory bag (feature #33). Keyed on `MemoryFact.key`
    /// so writes upsert and the snapshot can render a deduped list. Lives
    /// alongside `podcasts` in `podcasts.json` so both projections share
    /// one persistence pass.
    memory_facts: HashMap<String, MemoryFact>,
    /// Per-episode ad-break intervals keyed by the string form of
    /// `EpisodeId`. See [`mod@ad_segments`] for the accessor surface.
    pub(super) ad_segments: HashMap<String, Vec<AdSegment>>,
    /// AI Inbox triage decisions reported by iOS (M4 / D7). Keyed by the
    /// string form of `EpisodeId`; value is `(decision, is_hero, rationale)`
    /// where `decision` is `"inbox"` | `"archived"`. See [`mod@triage_state`].
    pub(super) episode_triage: HashMap<String, (String, bool, Option<String>)>,
    /// Episodes whose metadata (or transcript) chunk has been embedded into the
    /// RAG index, reported by iOS (M4 / D7). String form of `EpisodeId`.
    pub(super) metadata_indexed_episodes: HashSet<String>,
    /// Transient transcript-ingestion status overrides reported by iOS
    /// (M4 / D7). Keyed by the string form of `EpisodeId`; value is
    /// `(status, message)` where `status` is one of `"queued"` |
    /// `"fetching_publisher"` | `"transcribing"` | `"failed"`. `.ready` is
    /// derived from the stored `transcript`, never stored here.
    pub(super) transcript_status_overrides: HashMap<String, (String, Option<String>)>,
    /// Per-episode pipeline event log (download/transcript/identify lifecycle).
    /// Keyed by the string form of `EpisodeId`. Hydrated lazily per episode and
    /// persisted to its own `episode-events/<id>.json` file — never to the
    /// `podcasts.json` snapshot persist path. See [`mod@events`].
    pub(super) episode_events: events::EpisodeEventMap,
    /// User toggle: auto-skip ads when the playhead enters one.
    pub(super) auto_skip_ads_enabled: bool,
    /// When `true`, the kernel auto-advances to the next queued episode
    /// on `ItemEnd`. Default `true`.
    pub(super) auto_play_next: bool,
    /// When `true`, the kernel marks the episode listened on `ItemEnd`.
    /// Default `true`.
    pub(super) auto_mark_played_at_end: bool,
    /// Raw action string for headphone double-tap gesture.
    /// Default `"skip_forward"`. See `HeadphoneGestureAction` in Swift.
    pub(super) headphone_double_tap_action: String,
    /// Raw action string for headphone triple-tap gesture.
    /// Default `"clip_now"`. See `HeadphoneGestureAction` in Swift.
    pub(super) headphone_triple_tap_action: String,
    /// Skip-forward interval (seconds). Default 30.0; user-configurable.
    pub(super) skip_forward_secs: f64,
    /// Skip-backward interval (seconds). Default 15.0; user-configurable.
    pub(super) skip_backward_secs: f64,
    /// Default playback rate. Default 1.0; clamped to [0.5, 3.0].
    pub(super) default_playback_rate: f64,
    /// When `true`, downloaded files are deleted after the episode is marked played.
    pub(super) auto_delete_downloads_after_played: bool,
    /// LLM model ID for initial agent chat. E.g. "deepseek-v4-flash:cloud".
    /// Default: "deepseek-v4-flash:cloud".
    pub(super) agent_initial_model: String,
    /// Human-readable name for `agent_initial_model`. Default: "DeepSeek Flash".
    pub(super) agent_initial_model_name: String,
    /// LLM model ID for agent thinking/planning. E.g. "deepseek-v4-pro:cloud".
    /// Default: "deepseek-v4-pro:cloud".
    pub(super) agent_thinking_model: String,
    /// Human-readable name for `agent_thinking_model`. Default: "DeepSeek Pro".
    pub(super) agent_thinking_model_name: String,
    /// LLM model ID for memory compilation (agent memory synthesis).
    /// Default: "deepseek-v4-flash:cloud".
    pub(super) memory_compilation_model: String,
    /// Human-readable name for `memory_compilation_model`. Default: "DeepSeek Flash".
    pub(super) memory_compilation_model_name: String,
    /// LLM model ID for wiki synthesis. Default: "deepseek-v4-flash:cloud".
    pub(super) wiki_model: String,
    /// Human-readable name for `wiki_model`. Default: "DeepSeek Flash".
    pub(super) wiki_model_name: String,
    /// LLM model ID for episode categorization. Default: "deepseek-v4-flash:cloud".
    pub(super) categorization_model: String,
    /// Human-readable name for `categorization_model`. Default: "DeepSeek Flash".
    pub(super) categorization_model_name: String,
    /// LLM model ID for chapter compilation. Default: "deepseek-v4-flash:cloud".
    pub(super) chapter_compilation_model: String,
    /// Human-readable name for `chapter_compilation_model`. Default: "DeepSeek Flash".
    pub(super) chapter_compilation_model_name: String,
    /// LLM model ID for embeddings generation. Default: "openai/text-embedding-3-large".
    pub(super) embeddings_model: String,
    /// Human-readable name for `embeddings_model`. Default: "text-embedding-3-large".
    pub(super) embeddings_model_name: String,
    /// LLM model ID for image generation. Default: "google/gemini-2.5-flash-image".
    pub(super) image_generation_model: String,
    /// Human-readable name for `image_generation_model`. Default: "Gemini 2.5 Flash".
    pub(super) image_generation_model_name: String,
    /// Whether the reranker is enabled for search results. Default: `false`.
    pub(super) reranker_enabled: bool,
    pub(super) open_router_credential: ProviderCredentialMetadata,
    pub(super) ollama_credential: ProviderCredentialMetadata,
    /// Ollama chat endpoint URL for LLM inference.
    pub(super) ollama_chat_url: String,
    pub(super) eleven_labs_credential: ProviderCredentialMetadata,
    pub(super) assembly_ai_credential: ProviderCredentialMetadata,
    pub(super) perplexity_credential: ProviderCredentialMetadata,
    /// STT provider selection. Default `"apple_native"`.
    pub(super) stt_provider: String,
    /// OpenRouter Whisper model string. Default `"openai/whisper-1"`.
    pub(super) open_router_whisper_model: String,
    /// AssemblyAI STT model string. Default `"universal-3-pro,universal-2"`.
    pub(super) assembly_ai_stt_model: String,
    /// ElevenLabs STT model string. Default `"scribe_v1"`.
    pub(super) eleven_labs_stt_model: String,
    /// ElevenLabs TTS model string. Default `"eleven_turbo_v2_5"`.
    pub(super) eleven_labs_tts_model: String,
    /// ElevenLabs voice ID. Defaults to empty string.
    pub(super) eleven_labs_voice_id: String,
    /// ElevenLabs voice name. Defaults to empty string.
    pub(super) eleven_labs_voice_name: String,
    /// Blossom server URL. Default `"https://blossom.primal.net"`.
    pub(super) blossom_server_url: String,
    /// YouTube extractor URL (optional).
    pub(super) youtube_extractor_url: Option<String>,
    /// Local on-device LLM model ID (optional). When set, this dominates all callers
    /// in factory::backend_for, routing to LocalModelBackend via the global callback socket.
    pub(super) local_model_id: Option<String>,
    /// Whether to auto-generate wiki entries when transcripts are ingested. Default `false`.
    pub(super) wiki_auto_generate_on_transcript_ingest: bool,
    /// Whether to auto-ingest publisher-provided transcripts. Default `true`.
    pub(super) auto_ingest_publisher_transcripts: bool,
    /// Whether to fall back to Scribe (STT) when publisher transcript ingestion fails. Default `true`.
    pub(super) auto_fallback_to_scribe: bool,
    /// Whether to send local notifications when new episodes arrive. Default `true`.
    pub(super) notify_on_new_episodes: bool,
    /// Whether Nostr publishing and identity features are enabled. Default `false`.
    pub(super) nostr_enabled: bool,
    /// Primary Nostr relay URL for publishing and event distribution. Default empty.
    pub(super) nostr_relay_url: String,
    /// User's display name in Nostr profile metadata. Default empty.
    pub(super) nostr_profile_name: String,
    /// User's about/bio text in Nostr profile metadata. Default empty.
    pub(super) nostr_profile_about: String,
    /// User's picture URL in Nostr profile metadata. Default empty.
    pub(super) nostr_profile_picture: String,
    /// Nostr public key hex (read-only, derived from Keychain). Not persisted.
    pub(super) nostr_public_key_hex: Option<String>,
    /// Last-known Wi-Fi state reported by `nmp.network.capability`. `true` when
    /// the device's active interface is Wi-Fi. Defaults to `true` so
    /// auto-download runs on first launch before the iOS capability fires its
    /// initial `ConnectivityChanged` event (conservative: assumes Wi-Fi until
    /// told otherwise, avoiding unnecessary cellular charges on startup).
    /// Not persisted — refreshed from the capability on every app launch.
    pub(super) is_on_wifi: bool,
    /// Set of STT-provider raw values (`"elevenlabs_scribe"`, `"assemblyai"`,
    /// `"openrouter_whisper"`) whose API key is present in platform secure
    /// storage. Rust never holds the secret itself — platform hosts report
    /// presence via `podcast.settings.set_stt_keys_present`. This is the
    /// signal the kernel-owned STT fallback policy reads to decide whether a
    /// key-requiring provider can run or must downgrade to `apple_native`.
    /// Not persisted — hosts re-sync it from secure storage on every app launch.
    pub(super) stt_keys_present: std::collections::BTreeSet<String>,
    data_dir: Option<PathBuf>,
    /// Playback queue items loaded from disk during `set_data_dir`. Drained exactly
    /// once by `take_loaded_queue`; the FFI layer seeds the shared
    /// `PlaybackQueue` from this value after load completes.
    loaded_queue: Vec<QueuedPlaybackItem>,
    /// Current "Up Next" queue, mirrored here so that ordinary `persist()`
    /// calls (triggered by subscription changes, settings tweaks, etc.) write
    /// the real queue rather than an empty slice.  Updated by every
    /// `persist_with_queue` call and seeded from disk on `load_from_disk`.
    cached_queue: Vec<QueuedPlaybackItem>,
    /// OpenRouter API key (in-memory only, never persisted to disk).
    /// Set via `set_provider_api_keys`; credential never touches disk.
    open_router_api_key: Option<String>,
    /// Ollama API key (in-memory only, never persisted to disk).
    /// Set via `set_provider_api_keys`; credential never touches disk.
    ollama_api_key: Option<String>,
    /// ElevenLabs API key (in-memory only, never persisted to disk).
    /// Set via `set_provider_api_keys`; credential never touches disk.
    eleven_labs_api_key: Option<String>,
    /// AssemblyAI API key (in-memory only, never persisted to disk).
    /// Set via `set_provider_api_keys`; credential never touches disk.
    assembly_ai_api_key: Option<String>,
    /// Perplexity API key (in-memory only, never persisted to disk).
    /// Set via `set_provider_api_keys`; credential never touches disk.
    perplexity_api_key: Option<String>,
    /// User-curated podcast category labels. Keyed by PodcastId string;
    /// value is a Vec of free-form label strings (e.g. "AI", "News").
    /// Orthogonal to the AI-derived `CategoryBrowseItem` taxonomy.
    /// Persisted in `podcasts.json` under `podcast_user_categories`.
    pub(super) podcast_user_categories: HashMap<String, Vec<String>>,
}

impl PodcastStore {
    /// Canonical source of fresh-install settings defaults.
    ///
    /// This is one of the two — and only two — sites that may hold a literal
    /// settings default. The other is the Swift `SettingsSnapshot` property
    /// initializer mirror. Everything else (the `PersistedSettings` /
    /// `SettingsSnapshot` `Default` impls, the disk-hydration fallbacks, the
    /// Swift `Settings` defaults) derives from one of these two. A cross-language
    /// JSON fixture test (`tests/fixtures/settings_fresh_install.json`) enforces
    /// that the two stay in lockstep — change a default here and the fixture
    /// must be regenerated, which keeps Swift honest.
    pub fn new() -> Self {
        Self {
            podcasts: HashMap::new(),
            episodes: HashMap::new(),
            followed_podcasts: HashSet::new(),
            local_paths: HashMap::new(),
            file_sizes: HashMap::new(),
            transcripts: HashMap::new(),
            timed_transcripts: HashMap::new(),
            last_flushed_positions: HashMap::new(),
            has_completed_onboarding: false,
            auto_download_enabled: HashSet::new(),
            auto_download_modes: HashMap::new(),
            auto_download_cellular_allowed: HashSet::new(),
            transcription_disabled: HashSet::new(),
            notifications_disabled: HashSet::new(),
            pending_wifi_downloads: Vec::new(),
            memory_facts: HashMap::new(),
            ad_segments: HashMap::new(),
            episode_triage: HashMap::new(),
            metadata_indexed_episodes: HashSet::new(),
            transcript_status_overrides: HashMap::new(),
            episode_events: HashMap::new(),
            auto_skip_ads_enabled: true,
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
            embeddings_model: "openai/text-embedding-3-large".to_owned(),
            embeddings_model_name: "text-embedding-3-large".to_owned(),
            image_generation_model: "google/gemini-2.5-flash-image".to_owned(),
            image_generation_model_name: "Gemini 2.5 Flash".to_owned(),
            reranker_enabled: false,
            open_router_credential: ProviderCredentialMetadata::default(),
            ollama_credential: ProviderCredentialMetadata::default(),
            ollama_chat_url: DEFAULT_OLLAMA_CHAT_URL.to_owned(),
            eleven_labs_credential: ProviderCredentialMetadata::default(),
            assembly_ai_credential: ProviderCredentialMetadata::default(),
            perplexity_credential: ProviderCredentialMetadata::default(),
            stt_provider: "apple_native".to_owned(),
            open_router_whisper_model: "openai/whisper-1".to_owned(),
            assembly_ai_stt_model: "universal-3-pro,universal-2".to_owned(),
            eleven_labs_stt_model: "scribe_v1".to_owned(),
            eleven_labs_tts_model: "eleven_turbo_v2_5".to_owned(),
            eleven_labs_voice_id: String::new(),
            eleven_labs_voice_name: String::new(),
            blossom_server_url: "https://blossom.primal.net".to_owned(),
            youtube_extractor_url: None,
            local_model_id: None,
            wiki_auto_generate_on_transcript_ingest: false,
            auto_ingest_publisher_transcripts: true,
            auto_fallback_to_scribe: true,
            notify_on_new_episodes: true,
            nostr_enabled: false,
            nostr_relay_url: String::new(),
            nostr_profile_name: String::new(),
            nostr_profile_about: String::new(),
            nostr_profile_picture: String::new(),
            nostr_public_key_hex: None,
            is_on_wifi: true,
            stt_keys_present: std::collections::BTreeSet::new(),
            data_dir: None,
            loaded_queue: Vec::new(),
            cached_queue: Vec::new(),
            open_router_api_key: None,
            ollama_api_key: None,
            eleven_labs_api_key: None,
            assembly_ai_api_key: None,
            perplexity_api_key: None,
            podcast_user_categories: HashMap::new(),
        }
    }
}

impl Default for PodcastStore {
    fn default() -> Self {
        Self::new()
    }
}
