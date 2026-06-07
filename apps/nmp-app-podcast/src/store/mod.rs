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
use std::path::{Path, PathBuf};

use podcast_core::{Episode, EpisodeId, Podcast, PodcastId};

mod ad_segments;
pub mod auto_download;
mod chapters;
pub mod events;
pub mod identity;
pub mod inbox_triage_cache;
mod library;
mod memory;
pub(crate) mod owned_ext;
mod persistence;
mod playback;
pub mod podcast_keys;
pub mod relay_config;
mod settings;
pub mod stt_policy;
pub(crate) mod summary;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_ext;
mod transcripts;
mod triage_state;

use crate::ffi::projections::MemoryFact;
use crate::player::AdSegment;
pub use auto_download::episodes_to_auto_download;
use persistence::{PersistedPodcast, PersistedSettings, PersistedStore, PERSIST_SCHEMA_VERSION};
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
    auto_download_enabled: HashSet<PodcastId>,
    /// Podcasts for which cellular auto-download is **explicitly allowed**
    /// (i.e. the user set Wi-Fi-only to `false`). Absence means the default
    /// applies: Wi-Fi-only (matching `AutoDownloadPolicy.default.wifiOnly`).
    /// Cleared by `unsubscribe`.
    auto_download_cellular_allowed: HashSet<PodcastId>,
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
    /// LLM model ID for embeddings generation. Default: "deepseek-v4-flash:cloud".
    pub(super) embeddings_model: String,
    /// Human-readable name for `embeddings_model`. Default: "DeepSeek Flash".
    pub(super) embeddings_model_name: String,
    /// LLM model ID for image generation. Default: "google/gemini-2.5-flash-image".
    pub(super) image_generation_model: String,
    /// Human-readable name for `image_generation_model`. Default: "Gemini 2.5 Flash".
    pub(super) image_generation_model_name: String,
    /// Whether the reranker is enabled for search results. Default: `false`.
    pub(super) reranker_enabled: bool,
    /// OpenRouter credential source enum (raw String: "apiKey", "byok", "nostr").
    pub(super) open_router_credential_source: String,
    /// OpenRouter BYOK key ID (optional).
    pub(super) open_router_byok_key_id: Option<String>,
    /// OpenRouter BYOK key label (optional).
    pub(super) open_router_byok_key_label: Option<String>,
    /// OpenRouter credential connected-at timestamp (epoch seconds, optional).
    pub(super) open_router_connected_at: Option<i64>,
    /// Ollama credential source enum (raw String: "apiKey", "byok", "nostr").
    pub(super) ollama_credential_source: String,
    /// Ollama BYOK key ID (optional).
    pub(super) ollama_byok_key_id: Option<String>,
    /// Ollama BYOK key label (optional).
    pub(super) ollama_byok_key_label: Option<String>,
    /// Ollama credential connected-at timestamp (epoch seconds, optional).
    pub(super) ollama_connected_at: Option<i64>,
    /// Ollama chat endpoint URL for LLM inference.
    pub(super) ollama_chat_url: String,
    /// ElevenLabs credential source enum (raw String: "apiKey", "byok", "nostr").
    pub(super) eleven_labs_credential_source: String,
    /// ElevenLabs BYOK key ID (optional).
    pub(super) eleven_labs_byok_key_id: Option<String>,
    /// ElevenLabs BYOK key label (optional).
    pub(super) eleven_labs_byok_key_label: Option<String>,
    /// ElevenLabs credential connected-at timestamp (epoch seconds, optional).
    pub(super) eleven_labs_connected_at: Option<i64>,
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
    /// List of public Nostr relay URLs for broadcast and subscription. Default empty.
    pub(super) nostr_public_relays: Vec<String>,
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
    /// Episode ids loaded from disk during `set_data_dir`. Drained exactly
    /// once by `take_loaded_queue`; the FFI layer seeds the shared
    /// `PlaybackQueue` from this value after load completes.
    loaded_queue: Vec<String>,
    /// Current "Up Next" queue, mirrored here so that ordinary `persist()`
    /// calls (triggered by subscription changes, settings tweaks, etc.) write
    /// the real queue rather than an empty slice.  Updated by every
    /// `persist_with_queue` call and seeded from disk on `load_from_disk`.
    cached_queue: Vec<String>,
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
}

impl PodcastStore {
    pub fn new() -> Self {
        Self {
            podcasts: HashMap::new(),
            episodes: HashMap::new(),
            followed_podcasts: HashSet::new(),
            local_paths: HashMap::new(),
            file_sizes: HashMap::new(),
            transcripts: HashMap::new(),
            last_flushed_positions: HashMap::new(),
            has_completed_onboarding: false,
            auto_download_enabled: HashSet::new(),
            auto_download_cellular_allowed: HashSet::new(),
            pending_wifi_downloads: Vec::new(),
            memory_facts: HashMap::new(),
            ad_segments: HashMap::new(),
            episode_triage: HashMap::new(),
            metadata_indexed_episodes: HashSet::new(),
            transcript_status_overrides: HashMap::new(),
            episode_events: HashMap::new(),
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
            local_model_id: None,
            wiki_auto_generate_on_transcript_ingest: false,
            auto_ingest_publisher_transcripts: true,
            auto_fallback_to_scribe: true,
            notify_on_new_episodes: true,
            nostr_enabled: false,
            nostr_relay_url: String::new(),
            nostr_public_relays: Vec::new(),
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
        }
    }

    /// Bind the store to a persistence directory and load any existing state.
    ///
    /// Replaces the current in-memory contents with whatever `podcasts.json`
    /// inside `dir` contains (or leaves them empty when the file is absent /
    /// corrupted). The directory is created if missing.
    ///
    /// Returns the number of podcasts loaded so the FFI wrapper can decide
    /// whether to bump `rev` and force iOS to re-poll the snapshot.
    ///
    /// Idempotent: calling twice with the same path is safe; calling with a
    /// new path rebinds and re-loads.
    pub fn set_data_dir(&mut self, dir: PathBuf) -> usize {
        // create_dir_all is a no-op when the directory already exists.
        let _ = std::fs::create_dir_all(&dir);
        self.data_dir = Some(dir.clone());
        self.load_from_disk()
    }

    /// Reload from `data_dir/podcasts.json`. Returns the number of podcasts
    /// hydrated. Silent no-op when no data dir is set or the file is missing.
    fn load_from_disk(&mut self) -> usize {
        let Some(dir) = self.data_dir.as_ref() else {
            return 0;
        };
        let loaded = match persistence::load(dir) {
            Ok(Some(payload)) => payload,
            Ok(None) => return 0,
            Err(_) => return 0, // D6 — corrupted file ⇒ start fresh on next write
        };
        self.podcasts.clear();
        self.episodes.clear();
        self.followed_podcasts.clear();
        self.local_paths.clear();
        self.file_sizes.clear();
        self.transcripts.clear();
        // Hydrated episode positions are themselves the most-recent flushed
        // checkpoint: seed the throttling marker so the writeback layer
        // doesn't immediately re-flush on the next `Playing` tick.
        self.last_flushed_positions.clear();
        self.auto_download_enabled.clear();
        self.auto_download_cellular_allowed.clear();
        self.memory_facts.clear();
        self.ad_segments.clear();
        self.episode_triage.clear();
        self.metadata_indexed_episodes.clear();
        self.transcript_status_overrides.clear();
        for row in loaded.podcasts {
            let id = row.podcast.id;
            for ep in &row.episodes {
                if ep.position_secs > 0.0 {
                    self.last_flushed_positions
                        .insert(ep.id.0.to_string(), ep.position_secs);
                }
            }
            self.podcasts.insert(id, row.podcast);
            self.episodes.insert(id, row.episodes);
            if row.is_subscribed {
                self.followed_podcasts.insert(id);
            }
            if row.auto_download {
                self.auto_download_enabled.insert(id);
            }
            if row.cellular_allowed {
                self.auto_download_cellular_allowed.insert(id);
            }
        }
        // Settings are stored in the same envelope so onboarding completion
        // survives restart without a second file. `serde(default)` keeps
        // older saved files (predating the field) loading cleanly.
        self.has_completed_onboarding = loaded.has_completed_onboarding;
        for fact in loaded.memory_facts {
            self.memory_facts.insert(fact.key.clone(), fact);
        }
        for (ep_id, segs) in loaded.ad_segments {
            self.ad_segments.insert(ep_id, segs);
        }
        for (ep_id, decision, is_hero, rationale) in loaded.episode_triage {
            self.episode_triage
                .insert(ep_id, (decision, is_hero, rationale));
        }
        for ep_id in loaded.metadata_indexed_episodes {
            self.metadata_indexed_episodes.insert(ep_id);
        }
        for (ep_id, status, message) in loaded.transcript_status_overrides {
            self.transcript_status_overrides
                .insert(ep_id, (status, message));
        }
        self.auto_skip_ads_enabled = loaded.settings.auto_skip_ads_enabled;
        self.auto_play_next = loaded.settings.auto_play_next;
        self.auto_mark_played_at_end = loaded.settings.auto_mark_played_at_end;
        if !loaded.settings.headphone_double_tap_action.is_empty() {
            self.headphone_double_tap_action = loaded.settings.headphone_double_tap_action;
        }
        if !loaded.settings.headphone_triple_tap_action.is_empty() {
            self.headphone_triple_tap_action = loaded.settings.headphone_triple_tap_action;
        }
        // On-disk value of 0.0 means "field absent in old file" — replace
        // with the semantic default so the UI gets a usable value.
        self.skip_forward_secs = if loaded.settings.skip_forward_secs > 0.0 {
            loaded.settings.skip_forward_secs
        } else {
            30.0
        };
        self.skip_backward_secs = if loaded.settings.skip_backward_secs > 0.0 {
            loaded.settings.skip_backward_secs
        } else {
            15.0
        };
        self.default_playback_rate = if loaded.settings.default_playback_rate > 0.0 {
            loaded.settings.default_playback_rate
        } else {
            1.0
        };
        self.auto_delete_downloads_after_played =
            loaded.settings.auto_delete_downloads_after_played;
        // On-disk empty string means "field absent in old file" — replace with default.
        self.agent_initial_model = if !loaded.settings.agent_initial_model.is_empty() {
            loaded.settings.agent_initial_model
        } else {
            "deepseek-v4-flash:cloud".to_owned()
        };
        self.agent_initial_model_name = if !loaded.settings.agent_initial_model_name.is_empty() {
            loaded.settings.agent_initial_model_name
        } else {
            "DeepSeek Flash".to_owned()
        };
        self.agent_thinking_model = if !loaded.settings.agent_thinking_model.is_empty() {
            loaded.settings.agent_thinking_model
        } else {
            "deepseek-v4-pro:cloud".to_owned()
        };
        self.agent_thinking_model_name = if !loaded.settings.agent_thinking_model_name.is_empty() {
            loaded.settings.agent_thinking_model_name
        } else {
            "DeepSeek Pro".to_owned()
        };
        self.memory_compilation_model = if !loaded.settings.memory_compilation_model.is_empty() {
            loaded.settings.memory_compilation_model
        } else {
            "deepseek-v4-flash:cloud".to_owned()
        };
        self.memory_compilation_model_name =
            if !loaded.settings.memory_compilation_model_name.is_empty() {
                loaded.settings.memory_compilation_model_name
            } else {
                "DeepSeek Flash".to_owned()
            };
        self.wiki_model = if !loaded.settings.wiki_model.is_empty() {
            loaded.settings.wiki_model
        } else {
            "deepseek-v4-flash:cloud".to_owned()
        };
        self.wiki_model_name = if !loaded.settings.wiki_model_name.is_empty() {
            loaded.settings.wiki_model_name
        } else {
            "DeepSeek Flash".to_owned()
        };
        self.categorization_model = if !loaded.settings.categorization_model.is_empty() {
            loaded.settings.categorization_model
        } else {
            "deepseek-v4-flash:cloud".to_owned()
        };
        self.categorization_model_name = if !loaded.settings.categorization_model_name.is_empty() {
            loaded.settings.categorization_model_name
        } else {
            "DeepSeek Flash".to_owned()
        };
        self.chapter_compilation_model = if !loaded.settings.chapter_compilation_model.is_empty() {
            loaded.settings.chapter_compilation_model
        } else {
            "deepseek-v4-flash:cloud".to_owned()
        };
        self.chapter_compilation_model_name =
            if !loaded.settings.chapter_compilation_model_name.is_empty() {
                loaded.settings.chapter_compilation_model_name
            } else {
                "DeepSeek Flash".to_owned()
            };
        self.embeddings_model = if !loaded.settings.embeddings_model.is_empty() {
            loaded.settings.embeddings_model
        } else {
            "deepseek-v4-flash:cloud".to_owned()
        };
        self.embeddings_model_name = if !loaded.settings.embeddings_model_name.is_empty() {
            loaded.settings.embeddings_model_name
        } else {
            "DeepSeek Flash".to_owned()
        };
        self.image_generation_model = if !loaded.settings.image_generation_model.is_empty() {
            loaded.settings.image_generation_model
        } else {
            "google/gemini-2.5-flash-image".to_owned()
        };
        self.image_generation_model_name =
            if !loaded.settings.image_generation_model_name.is_empty() {
                loaded.settings.image_generation_model_name
            } else {
                "Gemini 2.5 Flash".to_owned()
            };
        self.reranker_enabled = loaded.settings.reranker_enabled;
        self.open_router_credential_source = loaded.settings.open_router_credential_source;
        self.open_router_byok_key_id = loaded.settings.open_router_byok_key_id;
        self.open_router_byok_key_label = loaded.settings.open_router_byok_key_label;
        self.open_router_connected_at = loaded.settings.open_router_connected_at;
        self.ollama_credential_source = loaded.settings.ollama_credential_source;
        self.ollama_byok_key_id = loaded.settings.ollama_byok_key_id;
        self.ollama_byok_key_label = loaded.settings.ollama_byok_key_label;
        self.ollama_connected_at = loaded.settings.ollama_connected_at;
        self.ollama_chat_url = loaded.settings.ollama_chat_url;
        self.eleven_labs_credential_source = loaded.settings.eleven_labs_credential_source;
        self.eleven_labs_byok_key_id = loaded.settings.eleven_labs_byok_key_id;
        self.eleven_labs_byok_key_label = loaded.settings.eleven_labs_byok_key_label;
        self.eleven_labs_connected_at = loaded.settings.eleven_labs_connected_at;
        self.stt_provider = if !loaded.settings.stt_provider.is_empty() {
            loaded.settings.stt_provider
        } else {
            "apple_native".to_owned()
        };
        self.open_router_whisper_model = if !loaded.settings.open_router_whisper_model.is_empty() {
            loaded.settings.open_router_whisper_model
        } else {
            "openai/whisper-1".to_owned()
        };
        self.assembly_ai_stt_model = if !loaded.settings.assembly_ai_stt_model.is_empty() {
            loaded.settings.assembly_ai_stt_model
        } else {
            "universal-3-pro,universal-2".to_owned()
        };
        self.eleven_labs_stt_model = if !loaded.settings.eleven_labs_stt_model.is_empty() {
            loaded.settings.eleven_labs_stt_model
        } else {
            "scribe_v1".to_owned()
        };
        self.eleven_labs_tts_model = if !loaded.settings.eleven_labs_tts_model.is_empty() {
            loaded.settings.eleven_labs_tts_model
        } else {
            "eleven_turbo_v2_5".to_owned()
        };
        self.eleven_labs_voice_id = loaded.settings.eleven_labs_voice_id;
        self.eleven_labs_voice_name = loaded.settings.eleven_labs_voice_name;
        self.blossom_server_url = if !loaded.settings.blossom_server_url.is_empty() {
            loaded.settings.blossom_server_url
        } else {
            "https://blossom.primal.net".to_owned()
        };
        self.youtube_extractor_url = loaded.settings.youtube_extractor_url;
        self.wiki_auto_generate_on_transcript_ingest =
            loaded.settings.wiki_auto_generate_on_transcript_ingest;
        self.auto_ingest_publisher_transcripts = loaded.settings.auto_ingest_publisher_transcripts;
        self.auto_fallback_to_scribe = loaded.settings.auto_fallback_to_scribe;
        self.notify_on_new_episodes = loaded.settings.notify_on_new_episodes;
        self.nostr_enabled = loaded.settings.nostr_enabled;
        self.nostr_relay_url = loaded.settings.nostr_relay_url;
        self.nostr_public_relays = loaded.settings.nostr_public_relays;
        self.nostr_profile_name = loaded.settings.nostr_profile_name;
        self.nostr_profile_about = loaded.settings.nostr_profile_about;
        self.nostr_profile_picture = loaded.settings.nostr_profile_picture;
        // nostr_public_key_hex is read-only (from Keychain), never hydrate from persisted state
        self.nostr_public_key_hex = None;
        self.cached_queue = loaded.queue.clone();
        self.loaded_queue = loaded.queue;
        // Restore deferred Wi-Fi downloads that were pending when the app was
        // last killed. These survive restart and are dispatched on the next
        // Wi-Fi connectivity event.
        self.pending_wifi_downloads = loaded.pending_wifi_downloads;
        self.podcasts.len()
    }

    /// Drain the queue snapshot that was hydrated by the most recent
    /// `set_data_dir` call. Returns an empty vec on all subsequent calls
    /// (and before any load). The FFI layer seeds `PlaybackQueue` from this
    /// value immediately after `set_data_dir` returns.
    pub fn take_loaded_queue(&mut self) -> Vec<String> {
        std::mem::take(&mut self.loaded_queue)
    }

    /// Flush the current in-memory state to `data_dir/podcasts.json`. Silent
    /// no-op when no data dir is set. Errors are intentionally swallowed
    /// (D6) — the in-memory store stays authoritative.
    pub(super) fn persist(&self) {
        let Some(dir) = self.data_dir.as_ref() else {
            return;
        };
        let mut payload = self.to_persisted();
        payload.queue = self.cached_queue.clone();
        let _ = persistence::save(dir, &payload);
    }

    /// Update the cached queue and flush to `data_dir/podcasts.json`. Called
    /// by the queue action handler after every mutation so the queue survives
    /// app restart. Silent no-op when no data dir is set (D6).
    pub(crate) fn persist_with_queue(&mut self, queue_items: &[String]) {
        self.cached_queue = queue_items.to_vec();
        self.persist();
    }

    fn to_persisted(&self) -> PersistedStore {
        let mut rows: Vec<PersistedPodcast> = self
            .podcasts
            .iter()
            .map(|(id, podcast)| PersistedPodcast {
                podcast: podcast.clone(),
                episodes: self.episodes.get(id).cloned().unwrap_or_default(),
                is_subscribed: self.followed_podcasts.contains(id),
                auto_download: self.auto_download_enabled.contains(id),
                cellular_allowed: self.auto_download_cellular_allowed.contains(id),
            })
            .collect();
        // Stable order so two consecutive saves produce identical bytes —
        // helps when diffing on-disk state during debugging.
        rows.sort_by(|a, b| a.podcast.id.0.cmp(&b.podcast.id.0));
        let mut facts: Vec<MemoryFact> = self.memory_facts.values().cloned().collect();
        // Same stable-order rationale as podcasts: keep saves byte-stable.
        facts.sort_by(|a, b| a.key.cmp(&b.key));
        let ad_segments: std::collections::BTreeMap<String, Vec<AdSegment>> = self
            .ad_segments
            .iter()
            .filter(|(_, v)| !v.is_empty())
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        // Sort the M4 side-maps on write (BTreeMap) so two consecutive saves
        // produce byte-identical output — same rationale as podcasts / facts /
        // ad_segments above.
        let episode_triage: Vec<(String, String, bool, Option<String>)> = self
            .episode_triage
            .iter()
            .collect::<std::collections::BTreeMap<_, _>>()
            .into_iter()
            .map(|(k, (decision, is_hero, rationale))| {
                (k.clone(), decision.clone(), *is_hero, rationale.clone())
            })
            .collect();
        let metadata_indexed_episodes: Vec<String> = self
            .metadata_indexed_episodes
            .iter()
            .cloned()
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        let transcript_status_overrides: Vec<(String, String, Option<String>)> = self
            .transcript_status_overrides
            .iter()
            .collect::<std::collections::BTreeMap<_, _>>()
            .into_iter()
            .map(|(k, (status, message))| (k.clone(), status.clone(), message.clone()))
            .collect();
        PersistedStore {
            schema_version: PERSIST_SCHEMA_VERSION,
            podcasts: rows,
            has_completed_onboarding: self.has_completed_onboarding,
            memory_facts: facts,
            ad_segments: ad_segments.into_iter().collect(),
            episode_triage,
            metadata_indexed_episodes,
            transcript_status_overrides,
            settings: PersistedSettings {
                auto_skip_ads_enabled: self.auto_skip_ads_enabled,
                auto_play_next: self.auto_play_next,
                auto_mark_played_at_end: self.auto_mark_played_at_end,
                headphone_double_tap_action: self.headphone_double_tap_action.clone(),
                headphone_triple_tap_action: self.headphone_triple_tap_action.clone(),
                skip_forward_secs: self.skip_forward_secs,
                skip_backward_secs: self.skip_backward_secs,
                default_playback_rate: self.default_playback_rate,
                auto_delete_downloads_after_played: self.auto_delete_downloads_after_played,
                agent_initial_model: self.agent_initial_model.clone(),
                agent_initial_model_name: self.agent_initial_model_name.clone(),
                agent_thinking_model: self.agent_thinking_model.clone(),
                agent_thinking_model_name: self.agent_thinking_model_name.clone(),
                memory_compilation_model: self.memory_compilation_model.clone(),
                memory_compilation_model_name: self.memory_compilation_model_name.clone(),
                wiki_model: self.wiki_model.clone(),
                wiki_model_name: self.wiki_model_name.clone(),
                categorization_model: self.categorization_model.clone(),
                categorization_model_name: self.categorization_model_name.clone(),
                chapter_compilation_model: self.chapter_compilation_model.clone(),
                chapter_compilation_model_name: self.chapter_compilation_model_name.clone(),
                embeddings_model: self.embeddings_model.clone(),
                embeddings_model_name: self.embeddings_model_name.clone(),
                image_generation_model: self.image_generation_model.clone(),
                image_generation_model_name: self.image_generation_model_name.clone(),
                reranker_enabled: self.reranker_enabled,
                open_router_credential_source: self.open_router_credential_source.clone(),
                open_router_byok_key_id: self.open_router_byok_key_id.clone(),
                open_router_byok_key_label: self.open_router_byok_key_label.clone(),
                open_router_connected_at: self.open_router_connected_at,
                ollama_credential_source: self.ollama_credential_source.clone(),
                ollama_byok_key_id: self.ollama_byok_key_id.clone(),
                ollama_byok_key_label: self.ollama_byok_key_label.clone(),
                ollama_connected_at: self.ollama_connected_at,
                ollama_chat_url: self.ollama_chat_url.clone(),
                eleven_labs_credential_source: self.eleven_labs_credential_source.clone(),
                eleven_labs_byok_key_id: self.eleven_labs_byok_key_id.clone(),
                eleven_labs_byok_key_label: self.eleven_labs_byok_key_label.clone(),
                eleven_labs_connected_at: self.eleven_labs_connected_at,
                stt_provider: self.stt_provider.clone(),
                open_router_whisper_model: self.open_router_whisper_model.clone(),
                assembly_ai_stt_model: self.assembly_ai_stt_model.clone(),
                eleven_labs_stt_model: self.eleven_labs_stt_model.clone(),
                eleven_labs_tts_model: self.eleven_labs_tts_model.clone(),
                eleven_labs_voice_id: self.eleven_labs_voice_id.clone(),
                eleven_labs_voice_name: self.eleven_labs_voice_name.clone(),
                blossom_server_url: self.blossom_server_url.clone(),
                youtube_extractor_url: self.youtube_extractor_url.clone(),
                local_model_id: self.local_model_id.clone(),
                wiki_auto_generate_on_transcript_ingest: self
                    .wiki_auto_generate_on_transcript_ingest,
                auto_ingest_publisher_transcripts: self.auto_ingest_publisher_transcripts,
                auto_fallback_to_scribe: self.auto_fallback_to_scribe,
                notify_on_new_episodes: self.notify_on_new_episodes,
                nostr_enabled: self.nostr_enabled,
                nostr_relay_url: self.nostr_relay_url.clone(),
                nostr_public_relays: self.nostr_public_relays.clone(),
                nostr_profile_name: self.nostr_profile_name.clone(),
                nostr_profile_about: self.nostr_profile_about.clone(),
                nostr_profile_picture: self.nostr_profile_picture.clone(),
                // nostr_public_key_hex is excluded from persistence (read-only, from Keychain)
                nostr_public_key_hex: None,
            },
            queue: Vec::new(), // filled by persist() from self.cached_queue after return
            pending_wifi_downloads: self.pending_wifi_downloads.clone(),
        }
    }

    /// Accessor for the currently-bound data dir, or `None` before
    /// `set_data_dir`. Read by the host-op handler's relay-edit arm to
    /// locate the relay-config sidecar (`relay_config::save_relay_config`).
    pub(crate) fn data_dir(&self) -> Option<&Path> {
        self.data_dir.as_deref()
    }
}

impl Default for PodcastStore {
    fn default() -> Self {
        Self::new()
    }
}
