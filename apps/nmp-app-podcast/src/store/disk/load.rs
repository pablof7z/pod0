//! Cold-start hydration: `PodcastStore::load_from_disk`.

use crate::clip_handler::ClipRecord;
use crate::llm::provider_config::normalize_ollama_chat_url;

use super::super::credential_metadata::ProviderCredentialMetadata;
use super::super::persistence::{self, PersistedSettings};
use super::super::PodcastStore;

impl PodcastStore {
    /// Reload from `data_dir/podcasts.json`. Returns the number of podcasts
    /// hydrated. Silent no-op when no data dir is set or the file is missing.
    pub(super) fn load_from_disk(&mut self) -> usize {
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
        self.timed_transcripts.clear();
        // Hydrated episode positions are themselves the most-recent flushed
        // checkpoint: seed the throttling marker so the writeback layer
        // doesn't immediately re-flush on the next `Playing` tick.
        self.last_flushed_positions.clear();
        self.auto_download_enabled.clear();
        self.auto_download_modes.clear();
        self.auto_download_cellular_allowed.clear();
        self.notifications_disabled.clear();
        self.memory_facts.clear();
        self.ad_segments.clear();
        self.clips.clear();
        self.episode_triage.clear();
        self.metadata_indexed_episodes.clear();
        self.transcript_status_overrides.clear();
        self.podcast_user_categories =
            loaded.podcast_user_categories.into_iter().collect();
        self.transcription_disabled.clear();
        for id_str in &loaded.transcription_disabled {
            if let Ok(uuid) = id_str.parse::<uuid::Uuid>() {
                self.transcription_disabled.insert(podcast_core::PodcastId::new(uuid));
            }
        }
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
            // Hydrate typed mode. `auto_download_mode` is new (additive field);
            // absent in older files means we fall back to the legacy bool:
            //   true  → AllNew  (matches the iOS default `.allNew`)
            //   false → Off
            let mode = row
                .auto_download_mode
                .unwrap_or_else(|| {
                    if row.auto_download {
                        crate::store::AutoDownloadMode::AllNew
                    } else {
                        crate::store::AutoDownloadMode::Off
                    }
                });
            if mode.is_enabled() {
                self.auto_download_enabled.insert(id);
                self.auto_download_modes.insert(id, mode);
            }
            if row.cellular_allowed {
                self.auto_download_cellular_allowed.insert(id);
            }
            if row.notifications_disabled {
                self.notifications_disabled.insert(id);
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
        // Hydrate persisted clips — convert PersistedClip → ClipRecord losslessly.
        self.clips = loaded.clips.into_iter().map(ClipRecord::from).collect();
        // Override with the Rust-owned clips sidecar (`clips.json`) when present.
        // `ClipHandler::persist_clips` writes this file directly; it is the
        // authoritative source for clips created after the last `podcasts.json`
        // flush. When the sidecar exists it supersedes whatever clips arrived
        // from `podcasts.json`.
        if let Some(dir) = self.data_dir.clone() {
            if let Some(sidecar) = crate::store::clip_records::load_clip_records(&dir) {
                self.clips = sidecar;
            }
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
        self.nostr_profile_name = loaded.settings.nostr_profile_name;
        self.nostr_profile_about = loaded.settings.nostr_profile_about;
        self.nostr_profile_picture = loaded.settings.nostr_profile_picture;
        // nostr_public_key_hex is read-only (from Keychain), never hydrate from persisted state
        self.nostr_public_key_hex = None;
        let loaded_queue: Vec<_> = loaded.queue.into_iter().map(Into::into).collect();
        self.cached_queue = loaded_queue.clone();
        self.loaded_queue = loaded_queue;
        // Restore deferred Wi-Fi downloads that were pending when the app was
        // last killed. These survive restart and are dispatched on the next
        // Wi-Fi connectivity event.
        self.pending_wifi_downloads = loaded.pending_wifi_downloads;
        self.podcasts.len()
    }
}
