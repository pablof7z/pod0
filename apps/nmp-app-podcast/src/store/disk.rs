//! Disk binding, hydration, and write-through persistence for `PodcastStore`.

use std::path::{Path, PathBuf};

use crate::ffi::projections::MemoryFact;
use crate::llm::provider_config::normalize_ollama_chat_url;
use crate::player::AdSegment;

use super::credential_metadata::ProviderCredentialMetadata;
use super::persistence::{
    self, PersistedPodcast, PersistedSettings, PersistedStore, PERSIST_SCHEMA_VERSION,
};
use super::PodcastStore;

impl PodcastStore {
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
        self.hydrate_download_maps(loaded.local_paths, loaded.file_sizes);
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
        // Canonical fallback values for fields whose on-disk sentinel (empty
        // string / 0.0) means "field absent in an old file". Sourced from
        // `PersistedSettings::default()`, which in turn derives from
        // `PodcastStore::new()` — so a default lives in exactly one place.
        let d = PersistedSettings::default();
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
            d.skip_forward_secs
        };
        self.skip_backward_secs = if loaded.settings.skip_backward_secs > 0.0 {
            loaded.settings.skip_backward_secs
        } else {
            d.skip_backward_secs
        };
        self.default_playback_rate = if loaded.settings.default_playback_rate > 0.0 {
            loaded.settings.default_playback_rate
        } else {
            d.default_playback_rate
        };
        self.auto_delete_downloads_after_played =
            loaded.settings.auto_delete_downloads_after_played;
        // On-disk empty string means "field absent in old file" — replace with default.
        self.agent_initial_model = if !loaded.settings.agent_initial_model.is_empty() {
            loaded.settings.agent_initial_model
        } else {
            d.agent_initial_model.clone()
        };
        self.agent_initial_model_name = if !loaded.settings.agent_initial_model_name.is_empty() {
            loaded.settings.agent_initial_model_name
        } else {
            d.agent_initial_model_name.clone()
        };
        self.agent_thinking_model = if !loaded.settings.agent_thinking_model.is_empty() {
            loaded.settings.agent_thinking_model
        } else {
            d.agent_thinking_model.clone()
        };
        self.agent_thinking_model_name = if !loaded.settings.agent_thinking_model_name.is_empty() {
            loaded.settings.agent_thinking_model_name
        } else {
            d.agent_thinking_model_name.clone()
        };
        self.memory_compilation_model = if !loaded.settings.memory_compilation_model.is_empty() {
            loaded.settings.memory_compilation_model
        } else {
            d.memory_compilation_model.clone()
        };
        self.memory_compilation_model_name =
            if !loaded.settings.memory_compilation_model_name.is_empty() {
                loaded.settings.memory_compilation_model_name
            } else {
                d.memory_compilation_model_name.clone()
            };
        self.wiki_model = if !loaded.settings.wiki_model.is_empty() {
            loaded.settings.wiki_model
        } else {
            d.wiki_model.clone()
        };
        self.wiki_model_name = if !loaded.settings.wiki_model_name.is_empty() {
            loaded.settings.wiki_model_name
        } else {
            d.wiki_model_name.clone()
        };
        self.categorization_model = if !loaded.settings.categorization_model.is_empty() {
            loaded.settings.categorization_model
        } else {
            d.categorization_model.clone()
        };
        self.categorization_model_name = if !loaded.settings.categorization_model_name.is_empty() {
            loaded.settings.categorization_model_name
        } else {
            d.categorization_model_name.clone()
        };
        self.chapter_compilation_model = if !loaded.settings.chapter_compilation_model.is_empty() {
            loaded.settings.chapter_compilation_model
        } else {
            d.chapter_compilation_model.clone()
        };
        self.chapter_compilation_model_name =
            if !loaded.settings.chapter_compilation_model_name.is_empty() {
                loaded.settings.chapter_compilation_model_name
            } else {
                d.chapter_compilation_model_name.clone()
            };
        self.embeddings_model = if !loaded.settings.embeddings_model.is_empty() {
            loaded.settings.embeddings_model
        } else {
            d.embeddings_model.clone()
        };
        self.embeddings_model_name = if !loaded.settings.embeddings_model_name.is_empty() {
            loaded.settings.embeddings_model_name
        } else {
            d.embeddings_model_name.clone()
        };
        self.image_generation_model = if !loaded.settings.image_generation_model.is_empty() {
            loaded.settings.image_generation_model
        } else {
            d.image_generation_model.clone()
        };
        self.image_generation_model_name =
            if !loaded.settings.image_generation_model_name.is_empty() {
                loaded.settings.image_generation_model_name
            } else {
                d.image_generation_model_name.clone()
            };
        self.reranker_enabled = loaded.settings.reranker_enabled;
        self.open_router_credential = ProviderCredentialMetadata::new(
            loaded.settings.open_router_credential_source,
            loaded.settings.open_router_byok_key_id,
            loaded.settings.open_router_byok_key_label,
            loaded.settings.open_router_connected_at,
        );
        self.ollama_credential = ProviderCredentialMetadata::new(
            loaded.settings.ollama_credential_source,
            loaded.settings.ollama_byok_key_id,
            loaded.settings.ollama_byok_key_label,
            loaded.settings.ollama_connected_at,
        );
        self.ollama_chat_url = normalize_ollama_chat_url(&loaded.settings.ollama_chat_url);
        self.eleven_labs_credential = ProviderCredentialMetadata::new(
            loaded.settings.eleven_labs_credential_source,
            loaded.settings.eleven_labs_byok_key_id,
            loaded.settings.eleven_labs_byok_key_label,
            loaded.settings.eleven_labs_connected_at,
        );
        self.assembly_ai_credential = ProviderCredentialMetadata::new(
            loaded.settings.assembly_ai_credential_source,
            loaded.settings.assembly_ai_byok_key_id,
            loaded.settings.assembly_ai_byok_key_label,
            loaded.settings.assembly_ai_connected_at,
        );
        self.perplexity_credential = ProviderCredentialMetadata::new(
            loaded.settings.perplexity_credential_source,
            loaded.settings.perplexity_byok_key_id,
            loaded.settings.perplexity_byok_key_label,
            loaded.settings.perplexity_connected_at,
        );
        self.stt_provider = if !loaded.settings.stt_provider.is_empty() {
            loaded.settings.stt_provider
        } else {
            d.stt_provider.clone()
        };
        self.open_router_whisper_model = if !loaded.settings.open_router_whisper_model.is_empty() {
            loaded.settings.open_router_whisper_model
        } else {
            d.open_router_whisper_model.clone()
        };
        self.assembly_ai_stt_model = if !loaded.settings.assembly_ai_stt_model.is_empty() {
            loaded.settings.assembly_ai_stt_model
        } else {
            d.assembly_ai_stt_model.clone()
        };
        self.eleven_labs_stt_model = if !loaded.settings.eleven_labs_stt_model.is_empty() {
            loaded.settings.eleven_labs_stt_model
        } else {
            d.eleven_labs_stt_model.clone()
        };
        self.eleven_labs_tts_model = if !loaded.settings.eleven_labs_tts_model.is_empty() {
            loaded.settings.eleven_labs_tts_model
        } else {
            d.eleven_labs_tts_model.clone()
        };
        self.eleven_labs_voice_id = loaded.settings.eleven_labs_voice_id;
        self.eleven_labs_voice_name = loaded.settings.eleven_labs_voice_name;
        self.blossom_server_url = if !loaded.settings.blossom_server_url.is_empty() {
            loaded.settings.blossom_server_url
        } else {
            d.blossom_server_url.clone()
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
            local_paths: self.persisted_local_paths(),
            file_sizes: self.persisted_file_sizes(),
            settings: self.persisted_settings(),
            queue: Vec::new(), // filled by persist() from self.cached_queue after return
            pending_wifi_downloads: self.pending_wifi_downloads.clone(),
        }
    }

    /// Project the live in-memory settings into the on-disk [`PersistedSettings`]
    /// envelope. Extracted from [`Self::to_persisted`] so the write path has one
    /// canonical settings-serialization site (mirroring the single canonical
    /// defaults site in [`super::PodcastStore::new`]).
    pub(super) fn persisted_settings(&self) -> PersistedSettings {
        PersistedSettings {
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
            open_router_credential_source: self.open_router_credential.source().to_owned(),
            open_router_byok_key_id: self.open_router_credential.byok_key_id_owned(),
            open_router_byok_key_label: self.open_router_credential.byok_key_label_owned(),
            open_router_connected_at: self.open_router_credential.connected_at(),
            ollama_credential_source: self.ollama_credential.source().to_owned(),
            ollama_byok_key_id: self.ollama_credential.byok_key_id_owned(),
            ollama_byok_key_label: self.ollama_credential.byok_key_label_owned(),
            ollama_connected_at: self.ollama_credential.connected_at(),
            ollama_chat_url: self.ollama_chat_url.clone(),
            eleven_labs_credential_source: self.eleven_labs_credential.source().to_owned(),
            eleven_labs_byok_key_id: self.eleven_labs_credential.byok_key_id_owned(),
            eleven_labs_byok_key_label: self.eleven_labs_credential.byok_key_label_owned(),
            eleven_labs_connected_at: self.eleven_labs_credential.connected_at(),
            assembly_ai_credential_source: self.assembly_ai_credential.source().to_owned(),
            assembly_ai_byok_key_id: self.assembly_ai_credential.byok_key_id_owned(),
            assembly_ai_byok_key_label: self.assembly_ai_credential.byok_key_label_owned(),
            assembly_ai_connected_at: self.assembly_ai_credential.connected_at(),
            perplexity_credential_source: self.perplexity_credential.source().to_owned(),
            perplexity_byok_key_id: self.perplexity_credential.byok_key_id_owned(),
            perplexity_byok_key_label: self.perplexity_credential.byok_key_label_owned(),
            perplexity_connected_at: self.perplexity_credential.connected_at(),
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
            wiki_auto_generate_on_transcript_ingest: self.wiki_auto_generate_on_transcript_ingest,
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
        }
    }

    /// Accessor for the currently-bound data dir, or `None` before
    /// `set_data_dir`. Read by the host-op handler's relay-edit arm to
    /// locate the relay-config sidecar (`relay_config::save_relay_config`).
    pub(crate) fn data_dir(&self) -> Option<&Path> {
        self.data_dir.as_deref()
    }
}
