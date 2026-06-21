//! Snapshot serialisation: `PodcastStore::to_persisted` and
//! `PodcastStore::persisted_settings`.

use crate::ffi::projections::MemoryFact;
use crate::player::AdSegment;

use super::super::persistence::{
    PersistedClip, PersistedPodcast, PersistedSettings, PersistedStore, PERSIST_SCHEMA_VERSION,
};
use super::super::PodcastStore;

impl PodcastStore {
    pub(super) fn to_persisted(&self) -> PersistedStore {
        let mut rows: Vec<PersistedPodcast> = self
            .podcasts
            .iter()
            .map(|(id, podcast)| PersistedPodcast {
                podcast: podcast.clone(),
                episodes: self.episodes.get(id).cloned().unwrap_or_default(),
                is_subscribed: self.followed_podcasts.contains(id),
                // Legacy bool kept for back-compat with older readers that
                // only understand the bool field.
                auto_download: self.auto_download_enabled.contains(id),
                // Typed mode for new readers. `None` if Off (omitted on wire
                // via `skip_serializing_if`).
                auto_download_mode: self
                    .auto_download_modes
                    .get(id)
                    .copied()
                    .filter(|m| m.is_enabled()),
                cellular_allowed: self.auto_download_cellular_allowed.contains(id),
                notifications_disabled: self.notifications_disabled.contains(id),
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
        // Note: clips are sorted by (created_at, id) in persistence::save()
        // so the on-disk bytes are always deterministic regardless of
        // insertion order. No sort needed here.
        let clips: Vec<PersistedClip> = self.clips.iter().map(PersistedClip::from).collect();
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
            podcast_user_categories: self
                .podcast_user_categories
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            transcription_disabled: self
                .transcription_disabled
                .iter()
                .map(|id| id.0.to_string())
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect(),
            clips,
        }
    }

    /// Project the live in-memory settings into the on-disk [`PersistedSettings`]
    /// envelope. Extracted from [`Self::to_persisted`] so the write path has one
    /// canonical settings-serialization site (mirroring the single canonical
    /// defaults site in [`super::super::PodcastStore::new`]).
    pub(in crate::store) fn persisted_settings(&self) -> PersistedSettings {
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
            nostr_profile_name: self.nostr_profile_name.clone(),
            nostr_profile_about: self.nostr_profile_about.clone(),
            nostr_profile_picture: self.nostr_profile_picture.clone(),
            // nostr_public_key_hex is excluded from persistence (read-only, from Keychain)
            nostr_public_key_hex: None,
        }
    }
}
