//! Disk persistence for [`PodcastStore`].
//!
//! Single JSON file (`podcasts.json`) inside a caller-supplied data directory.
//! Writes are atomic (write to `podcasts.json.tmp` then rename); failures
//! degrade silently per D6 — the in-memory store stays authoritative.
//!
//! ## Wire format
//!
//! ```text
//! {
//!   "schema_version": 1,
//!   "podcasts": [
//!     { "podcast": <Podcast>, "episodes": [<Episode>, ...], "is_subscribed": true }
//!   ],
//!   "memory_facts": [ { "id": "...", "key": "...", ... }, ... ]  // optional
//! }
//! ```
//!
//! Versioned so future migrations can detect older payloads. Unknown
//! schema_version is treated as "empty" — the file is replaced on next
//! write. New optional fields (e.g. `memory_facts` added in feature #33)
//! are tagged `#[serde(default)]` so older payloads decode cleanly without
//! bumping the schema and wiping every subscription on upgrade.

use std::path::{Path, PathBuf};

use podcast_core::{Episode, Podcast};
use serde::{Deserialize, Serialize};

use crate::ffi::projections::MemoryFact;
use crate::player::AdSegment;

/// Schema marker for `podcasts.json`. Bump on incompatible format changes.
pub const PERSIST_SCHEMA_VERSION: u32 = 1;

/// File name of the persisted store inside the data directory.
pub const PODCASTS_FILE: &str = "podcasts.json";

/// On-disk envelope. One row per known podcast with its episodes and follow
/// membership inlined so the load is a single fread.
///
/// `has_completed_onboarding` is part of the same envelope so the iOS
/// shell's `OnboardingView` gate survives restart without a second file.
/// `serde(default)` keeps older saved files (predating the field) loading
/// cleanly as `false`.
/// All fields except `schema_version` and `podcasts` use `#[serde(default)]`
/// so older saved files (pre-dating a field) load without errors.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(super) struct PersistedStore {
    pub schema_version: u32,
    pub podcasts: Vec<PersistedPodcast>,
    #[serde(default)]
    pub has_completed_onboarding: bool,
    /// Agent memory bag — optional so pre-v2 files decode cleanly.
    #[serde(default)]
    pub memory_facts: Vec<MemoryFact>,
    /// Per-episode ad-break intervals. Sorted on write for deterministic bytes.
    #[serde(default)]
    pub ad_segments: Vec<(String, Vec<AdSegment>)>,
    /// AI Inbox triage decisions (M4 / D7). Tuples of
    /// `(episode_id, decision, is_hero, rationale)`. Sorted on write for
    /// deterministic bytes. `#[serde(default)]` so pre-M4 files load as empty.
    #[serde(default)]
    pub episode_triage: Vec<(String, String, bool, Option<String>)>,
    /// Episodes covered by the RAG metadata index (M4 / D7). Sorted on write.
    /// `#[serde(default)]` so pre-M4 files load as empty.
    #[serde(default)]
    pub metadata_indexed_episodes: Vec<String>,
    /// Transient transcript-ingestion status overrides (M4 / D7). Tuples of
    /// `(episode_id, status, message)`. Sorted on write. `#[serde(default)]`
    /// so pre-M4 files load as empty.
    #[serde(default)]
    pub transcript_status_overrides: Vec<(String, String, Option<String>)>,
    /// Downloaded enclosure paths keyed by episode id. Stored outside the
    /// episode rows so feed refreshes cannot wipe downloaded-state.
    #[serde(default)]
    pub local_paths: Vec<(String, String)>,
    /// Downloaded enclosure sizes keyed by episode id. Lifecycle-locked to
    /// `local_paths`; missing or stale entries hydrate as unknown size.
    #[serde(default)]
    pub file_sizes: Vec<(String, i64)>,
    #[serde(default)]
    pub settings: PersistedSettings,
    /// "Up Next" queue — episode ids in play order. `#[serde(default)]` keeps
    /// pre-existing files (before queue persistence shipped) loading as empty.
    #[serde(default)]
    pub queue: Vec<String>,
    /// Episodes deferred because the device was on cellular at refresh time for
    /// a Wi-Fi-only show. Pairs of `(episode_id_str, enclosure_url)`. Persisted
    /// so that an app kill while on cellular doesn't permanently lose the
    /// deferred downloads — they survive restart and are dispatched on the next
    /// Wi-Fi connection. `#[serde(default)]` for backward compat with older
    /// files that lack this field.
    #[serde(default)]
    pub pending_wifi_downloads: Vec<(String, String)>,
}

/// On-disk settings envelope.
///
/// Carries `#[serde(default)]` at the container level, so any field absent
/// from an older `podcasts.json` hydrates from this struct's [`Default`] impl
/// — which itself derives from the single canonical defaults site,
/// [`super::PodcastStore::new`] (via [`super::PodcastStore::persisted_settings`]).
/// There are intentionally **no** per-field default literals here: the absent ⇒
/// canonical-default behavior is uniform across every field.
///
/// Note: several string fields additionally carry a *sentinel* hydration rule
/// in `load_from_disk` (empty string / 0.0 ⇒ canonical default), which guards
/// against pre-`#[serde(default)]` files that wrote explicit empties. Those
/// fallbacks also read from this `Default`, so the canonical value still lives
/// in exactly one place.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct PersistedSettings {
    /// Mirrors `PodcastStore::auto_skip_ads_enabled`.
    /// Absent ⇒ canonical default from `PodcastStore::new()`.
    pub auto_skip_ads_enabled: bool,
    /// When `true`, kernel auto-advances to the next queued episode on `ItemEnd`.
    /// Absent ⇒ canonical default from `PodcastStore::new()`.
    pub auto_play_next: bool,
    /// When `true`, kernel marks the episode listened on `ItemEnd`.
    /// Absent ⇒ canonical default from `PodcastStore::new()`.
    pub auto_mark_played_at_end: bool,
    /// Raw headphone double-tap action string. Empty string in old files →
    /// hydration replaces with the canonical default from `PodcastStore::new()`.
    pub headphone_double_tap_action: String,
    /// Raw headphone triple-tap action string. Empty string in old files →
    /// hydration replaces with the canonical default from `PodcastStore::new()`.
    pub headphone_triple_tap_action: String,
    /// Skip-forward interval in seconds. 0.0 in old files → hydration replaces
    /// with the canonical default from `PodcastStore::new()`.
    pub skip_forward_secs: f64,
    /// Skip-backward interval in seconds. Same 0.0 → canonical-default sentinel logic.
    pub skip_backward_secs: f64,
    /// Default playback rate. 0.0 in old files → hydration replaces with the
    /// canonical default from `PodcastStore::new()`.
    pub default_playback_rate: f64,
    /// When `true`, downloaded files are deleted after the episode is marked played.
    pub auto_delete_downloads_after_played: bool,
    /// LLM model ID for initial agent chat. Empty string in old files →
    /// hydration replaces with the canonical default from `PodcastStore::new()`.
    pub agent_initial_model: String,
    /// Human-readable name for agent initial model. Empty string in old files →
    /// hydration replaces with the canonical default from `PodcastStore::new()`.
    pub agent_initial_model_name: String,
    /// LLM model ID for agent thinking/planning. Empty string in old files →
    /// hydration replaces with the canonical default from `PodcastStore::new()`.
    pub agent_thinking_model: String,
    /// Human-readable name for agent thinking model. Empty string in old files →
    /// hydration replaces with the canonical default from `PodcastStore::new()`.
    pub agent_thinking_model_name: String,
    /// LLM model ID for memory compilation. Empty string in old files →
    /// hydration replaces with the canonical default from `PodcastStore::new()`.
    pub memory_compilation_model: String,
    /// Human-readable name for memory compilation model. Empty string in old files →
    /// hydration replaces with the canonical default from `PodcastStore::new()`.
    pub memory_compilation_model_name: String,
    /// LLM model ID for wiki synthesis. Empty string in old files →
    /// hydration replaces with the canonical default from `PodcastStore::new()`.
    pub wiki_model: String,
    /// Human-readable name for wiki model. Empty string in old files →
    /// hydration replaces with the canonical default from `PodcastStore::new()`.
    pub wiki_model_name: String,
    /// LLM model ID for episode categorization. Empty string in old files →
    /// hydration replaces with the canonical default from `PodcastStore::new()`.
    pub categorization_model: String,
    /// Human-readable name for categorization model. Empty string in old files →
    /// hydration replaces with the canonical default from `PodcastStore::new()`.
    pub categorization_model_name: String,
    /// LLM model ID for chapter compilation. Empty string in old files →
    /// hydration replaces with the canonical default from `PodcastStore::new()`.
    pub chapter_compilation_model: String,
    /// Human-readable name for chapter compilation model. Empty string in old files →
    /// hydration replaces with the canonical default from `PodcastStore::new()`.
    pub chapter_compilation_model_name: String,
    /// LLM model ID for embeddings generation. Empty string in old files →
    /// hydration replaces with the canonical default from `PodcastStore::new()`.
    pub embeddings_model: String,
    /// Human-readable name for embeddings model. Empty string in old files →
    /// hydration replaces with the canonical default from `PodcastStore::new()`.
    pub embeddings_model_name: String,
    /// LLM model ID for image generation. Empty string in old files →
    /// hydration replaces with the canonical default from `PodcastStore::new()`.
    pub image_generation_model: String,
    /// Human-readable name for image generation model. Empty string in old files →
    /// hydration replaces with the canonical default from `PodcastStore::new()`.
    pub image_generation_model_name: String,
    /// Whether the reranker is enabled for search results.
    pub reranker_enabled: bool,
    /// OpenRouter credential source enum (raw String: "apiKey", "byok", "nostr").
    pub open_router_credential_source: String,
    /// OpenRouter BYOK key ID (optional).
    pub open_router_byok_key_id: Option<String>,
    /// OpenRouter BYOK key label (optional).
    pub open_router_byok_key_label: Option<String>,
    /// OpenRouter credential connected-at timestamp (epoch seconds, optional).
    pub open_router_connected_at: Option<i64>,
    /// Ollama credential source enum (raw String: "apiKey", "byok", "nostr").
    pub ollama_credential_source: String,
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
    /// ElevenLabs BYOK key ID (optional).
    pub eleven_labs_byok_key_id: Option<String>,
    /// ElevenLabs BYOK key label (optional).
    pub eleven_labs_byok_key_label: Option<String>,
    /// ElevenLabs credential connected-at timestamp (epoch seconds, optional).
    pub eleven_labs_connected_at: Option<i64>,
    /// AssemblyAI credential metadata; secrets stay in platform secure storage.
    pub assembly_ai_credential_source: String,
    pub assembly_ai_byok_key_id: Option<String>,
    pub assembly_ai_byok_key_label: Option<String>,
    pub assembly_ai_connected_at: Option<i64>,
    /// Perplexity credential metadata; secrets stay in platform secure storage.
    pub perplexity_credential_source: String,
    pub perplexity_byok_key_id: Option<String>,
    pub perplexity_byok_key_label: Option<String>,
    pub perplexity_connected_at: Option<i64>,
    /// STT provider selection. Empty string in old files →
    /// hydration replaces with the canonical default from `PodcastStore::new()`.
    pub stt_provider: String,
    /// OpenRouter Whisper model string. Empty string in old files →
    /// hydration replaces with the canonical default from `PodcastStore::new()`.
    pub open_router_whisper_model: String,
    /// AssemblyAI STT model string. Empty string in old files →
    /// hydration replaces with the canonical default from `PodcastStore::new()`.
    pub assembly_ai_stt_model: String,
    /// ElevenLabs STT model string. Empty string in old files →
    /// hydration replaces with the canonical default from `PodcastStore::new()`.
    pub eleven_labs_stt_model: String,
    /// ElevenLabs TTS model string. Empty string in old files →
    /// hydration replaces with the canonical default from `PodcastStore::new()`.
    pub eleven_labs_tts_model: String,
    /// ElevenLabs voice ID. Defaults to empty string.
    pub eleven_labs_voice_id: String,
    /// ElevenLabs voice name. Defaults to empty string.
    pub eleven_labs_voice_name: String,
    /// Blossom server URL. Empty string in old files →
    /// hydration replaces with the canonical default from `PodcastStore::new()`.
    pub blossom_server_url: String,
    /// YouTube extractor URL (optional).
    pub youtube_extractor_url: Option<String>,
    /// Local on-device LLM model ID (optional).
    pub local_model_id: Option<String>,
    /// Whether to auto-generate wiki entries when transcripts are ingested.
    pub wiki_auto_generate_on_transcript_ingest: bool,
    /// Whether to auto-ingest publisher-provided transcripts.
    /// Absent ⇒ canonical default from `PodcastStore::new()`.
    pub auto_ingest_publisher_transcripts: bool,
    /// Whether to fall back to Scribe (STT) when publisher transcript ingestion fails.
    /// Absent ⇒ canonical default from `PodcastStore::new()`.
    pub auto_fallback_to_scribe: bool,
    /// Whether to send local notifications when new episodes arrive.
    /// Absent ⇒ canonical default from `PodcastStore::new()`.
    pub notify_on_new_episodes: bool,
    /// Whether Nostr publishing and identity features are enabled.
    pub nostr_enabled: bool,
    /// Primary Nostr relay URL for publishing and event distribution. Default empty.
    pub nostr_relay_url: String,
    /// List of public Nostr relay URLs for broadcast and subscription. Default empty.
    pub nostr_public_relays: Vec<String>,
    /// User's display name in Nostr profile metadata. Default empty.
    pub nostr_profile_name: String,
    /// User's about/bio text in Nostr profile metadata. Default empty.
    pub nostr_profile_about: String,
    /// User's picture URL in Nostr profile metadata. Default empty.
    pub nostr_profile_picture: String,
    /// Nostr public key hex (read-only, derived from Keychain). Not persisted.
    pub nostr_public_key_hex: Option<String>,
}

fn default_true() -> bool {
    true
}

impl Default for PersistedSettings {
    /// Derive every default from the single canonical defaults site,
    /// [`super::PodcastStore::new`]. There are no literals here on purpose —
    /// changing a fresh-install default is a one-line change in `new()`.
    fn default() -> Self {
        crate::store::PodcastStore::new().persisted_settings()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct PersistedPodcast {
    pub podcast: Podcast,
    #[serde(default)]
    pub episodes: Vec<Episode>,
    /// Explicit follow membership. Older persisted files predate the
    /// known-vs-subscribed split and every row in them was treated as followed,
    /// so the compatible default is `true`.
    #[serde(default = "default_true")]
    pub is_subscribed: bool,
    /// Per-podcast auto-download opt-in flag. `#[serde(default)]` lets the
    /// load path tolerate older `podcasts.json` files written before this
    /// field shipped: missing key ⇒ `false` (auto-download off). We
    /// deliberately do NOT bump `PERSIST_SCHEMA_VERSION` for this addition
    /// — bumping wipes the user's library because `load()` treats unknown
    /// schemas as empty (see this file, line ~60).
    #[serde(default)]
    pub auto_download: bool,
    /// When `true`, the user explicitly allowed cellular auto-downloads
    /// for this show (i.e. Wi-Fi-only is off). Absent in older files ⇒
    /// `false` (cellular not allowed — default Wi-Fi-only behaviour).
    #[serde(default)]
    pub cellular_allowed: bool,
}

/// Resolve the path of `podcasts.json` inside `data_dir`.
pub(super) fn podcasts_path(data_dir: &Path) -> PathBuf {
    data_dir.join(PODCASTS_FILE)
}

/// Load `podcasts.json` from `data_dir`. Returns `Ok(None)` when the file
/// does not exist (fresh install). Any parse / IO error is propagated so the
/// caller can decide whether to log and continue with an empty store.
pub(super) fn load(data_dir: &Path) -> std::io::Result<Option<PersistedStore>> {
    let path = podcasts_path(data_dir);
    match std::fs::read(&path) {
        Ok(bytes) => {
            let store: PersistedStore = serde_json::from_slice(&bytes)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
            if store.schema_version != PERSIST_SCHEMA_VERSION {
                // Unknown / future schema — treat as empty; the next mutation
                // will overwrite with the current shape.
                return Ok(None);
            }
            Ok(Some(store))
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err),
    }
}

/// Atomically write `payload` to `podcasts.json` inside `data_dir`.
///
/// Strategy: serialize → write to `podcasts.json.tmp` → `fs::rename` over the
/// final path. `rename` is atomic on the same filesystem, so the only failure
/// modes are "old file intact" or "new file in place" — never a partial write.
pub(super) fn save(data_dir: &Path, payload: &PersistedStore) -> std::io::Result<()> {
    // Ensure the directory exists. `create_dir_all` is a no-op when present.
    std::fs::create_dir_all(data_dir)?;

    let json = serde_json::to_vec_pretty(payload)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

    let final_path = podcasts_path(data_dir);
    let tmp_path = data_dir.join(format!("{PODCASTS_FILE}.tmp"));
    std::fs::write(&tmp_path, &json)?;
    std::fs::rename(&tmp_path, &final_path)?;
    Ok(())
}

// Tests split into persistence_tests.rs; #[path] keeps private items in scope.
#[cfg(test)]
#[path = "persistence_tests.rs"]
mod tests;
