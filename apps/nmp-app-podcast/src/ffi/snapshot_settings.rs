//! [`SettingsSnapshot`] assembly for the podcast update.
//!
//! Split out of `snapshot.rs` (the per-tick [`super::snapshot::build_podcast_update`]
//! builder) to keep that file under the 500-line hard limit. This is the single
//! ~80-field projection that maps the kernel `PodcastStore` settings accessors
//! into the wire-facing [`SettingsSnapshot`]; it is the largest self-contained
//! sub-builder in the snapshot path, so it owns its own file.

use super::projections::SettingsSnapshot;
use super::snapshot::provider_key_present;
use crate::llm::provider_config::normalize_ollama_chat_url;
use crate::store::PodcastStore;

/// Project the kernel store's settings accessors into the wire-facing
/// [`SettingsSnapshot`]. Called under the single store lock inside
/// [`super::snapshot::build_podcast_update`] so it never re-locks.
pub(super) fn build_settings_snapshot(s: &PodcastStore) -> SettingsSnapshot {
    SettingsSnapshot {
        has_completed_onboarding: s.has_completed_onboarding(),
        auto_skip_ads_enabled: s.auto_skip_ads_enabled(),
        auto_play_next: s.auto_play_next(),
        auto_mark_played_at_end: s.auto_mark_played_at_end(),
        headphone_double_tap_action: s.headphone_double_tap_action().to_owned(),
        headphone_triple_tap_action: s.headphone_triple_tap_action().to_owned(),
        skip_forward_secs: s.skip_forward_secs(),
        skip_backward_secs: s.skip_backward_secs(),
        default_playback_rate: s.default_playback_rate(),
        auto_delete_downloads_after_played: s.auto_delete_downloads_after_played(),
        agent_initial_model: s.agent_initial_model().to_owned(),
        agent_initial_model_name: s.agent_initial_model_name().to_owned(),
        agent_thinking_model: s.agent_thinking_model().to_owned(),
        agent_thinking_model_name: s.agent_thinking_model_name().to_owned(),
        memory_compilation_model: s.memory_compilation_model().to_owned(),
        memory_compilation_model_name: s.memory_compilation_model_name().to_owned(),
        wiki_model: s.wiki_model().to_owned(),
        wiki_model_name: s.wiki_model_name().to_owned(),
        categorization_model: s.categorization_model().to_owned(),
        categorization_model_name: s.categorization_model_name().to_owned(),
        chapter_compilation_model: s.chapter_compilation_model().to_owned(),
        chapter_compilation_model_name: s.chapter_compilation_model_name().to_owned(),
        embeddings_model: s.embeddings_model().to_owned(),
        embeddings_model_name: s.embeddings_model_name().to_owned(),
        image_generation_model: s.image_generation_model().to_owned(),
        image_generation_model_name: s.image_generation_model_name().to_owned(),
        reranker_enabled: s.reranker_enabled(),
        open_router_credential_source: s.open_router_credential_source().to_owned(),
        open_router_key_present: provider_key_present(s.open_router_api_key()),
        open_router_byok_key_id: s.open_router_byok_key_id().map(|s| s.to_owned()),
        open_router_byok_key_label: s.open_router_byok_key_label().map(|s| s.to_owned()),
        open_router_connected_at: s.open_router_connected_at(),
        ollama_credential_source: s.ollama_credential_source().to_owned(),
        ollama_key_present: provider_key_present(s.ollama_api_key()),
        ollama_byok_key_id: s.ollama_byok_key_id().map(|s| s.to_owned()),
        ollama_byok_key_label: s.ollama_byok_key_label().map(|s| s.to_owned()),
        ollama_connected_at: s.ollama_connected_at(),
        ollama_chat_url: normalize_ollama_chat_url(s.ollama_chat_url()),
        eleven_labs_credential_source: s.eleven_labs_credential_source().to_owned(),
        eleven_labs_key_present: provider_key_present(s.eleven_labs_api_key()),
        eleven_labs_byok_key_id: s.eleven_labs_byok_key_id().map(|s| s.to_owned()),
        eleven_labs_byok_key_label: s.eleven_labs_byok_key_label().map(|s| s.to_owned()),
        eleven_labs_connected_at: s.eleven_labs_connected_at(),
        assembly_ai_credential_source: s.assembly_ai_credential_source().to_owned(),
        assembly_ai_byok_key_id: s.assembly_ai_byok_key_id().map(|s| s.to_owned()),
        assembly_ai_byok_key_label: s.assembly_ai_byok_key_label().map(|s| s.to_owned()),
        assembly_ai_connected_at: s.assembly_ai_connected_at(),
        perplexity_credential_source: s.perplexity_credential_source().to_owned(),
        perplexity_byok_key_id: s.perplexity_byok_key_id().map(|s| s.to_owned()),
        perplexity_byok_key_label: s.perplexity_byok_key_label().map(|s| s.to_owned()),
        perplexity_connected_at: s.perplexity_connected_at(),
        stt_provider: s.stt_provider().to_owned(),
        effective_stt_provider: s.effective_stt_provider().to_owned(),
        effective_stt_provider_requires_key: crate::store::stt_policy::requires_key(
            s.effective_stt_provider(),
        ),
        assembly_ai_key_present: provider_key_present(s.assembly_ai_api_key()),
        perplexity_key_present: provider_key_present(s.perplexity_api_key()),
        open_router_whisper_model: s.open_router_whisper_model().to_owned(),
        assembly_ai_stt_model: s.assembly_ai_stt_model().to_owned(),
        eleven_labs_stt_model: s.eleven_labs_stt_model().to_owned(),
        eleven_labs_tts_model: s.eleven_labs_tts_model().to_owned(),
        eleven_labs_voice_id: s.eleven_labs_voice_id().to_owned(),
        eleven_labs_voice_name: s.eleven_labs_voice_name().to_owned(),
        blossom_server_url: s.blossom_server_url().to_owned(),
        youtube_extractor_url: s.youtube_extractor_url().map(|s| s.to_owned()),
        local_model_id: s.local_model_id().map(|s| s.to_owned()),
        wiki_auto_generate_on_transcript_ingest: s.wiki_auto_generate_on_transcript_ingest(),
        auto_ingest_publisher_transcripts: s.auto_ingest_publisher_transcripts(),
        auto_fallback_to_scribe: s.auto_fallback_to_scribe(),
        notify_on_new_episodes: s.notify_on_new_episodes(),
        nostr_enabled: s.nostr_enabled(),
        nostr_relay_url: s.nostr_relay_url().to_owned(),
        nostr_public_relays: s.nostr_public_relays().to_vec(),
        nostr_profile_name: s.nostr_profile_name().to_owned(),
        nostr_profile_about: s.nostr_profile_about().to_owned(),
        nostr_profile_picture: s.nostr_profile_picture().to_owned(),
        nostr_public_key_hex: s.nostr_public_key_hex().map(|s| s.to_owned()),
    }
}
