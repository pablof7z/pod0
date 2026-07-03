import Darwin
import Foundation

enum PodcastBridgeCompat {
    private nonisolated(unsafe) static var current: PodcastHandle?

    static func install(_ handle: PodcastHandle) {
        current = handle
    }

    static func uninstall(_ handle: PodcastHandle) {
        if current === handle {
            current = nil
        }
    }

    static func cString(_ value: String?) -> UnsafeMutablePointer<CChar>? {
        guard let value else { return nil }
        return value.withCString { strdup($0) }
    }

    static func string(_ ptr: UnsafePointer<CChar>?) -> String? {
        guard let ptr else { return nil }
        return String(cString: ptr)
    }

    static func app(for handle: UnsafeMutableRawPointer?) -> PodcastApp? {
        guard let current, let handle, handle == current.podcastHandle else { return nil }
        return current.podcastApp
    }
}

func nmp_free_string(_ ptr: UnsafeMutablePointer<CChar>?) {
    guard let ptr else { return }
    free(ptr)
}

private func podcastCall(
    _ handle: UnsafeMutableRawPointer?,
    _ endpoint: String,
    _ request: UnsafePointer<CChar>?
) -> UnsafeMutablePointer<CChar>? {
    guard let app = PodcastBridgeCompat.app(for: handle),
          let request = PodcastBridgeCompat.string(request)
    else { return nil }
    return PodcastBridgeCompat.cString(
        app.podcastBridgeCall(endpoint: endpoint, requestJson: request))
}

private func podcastCall(
    _ handle: UnsafeMutableRawPointer?,
    _ endpoint: String
) -> UnsafeMutablePointer<CChar>? {
    guard let app = PodcastBridgeCompat.app(for: handle) else { return nil }
    return PodcastBridgeCompat.cString(
        app.podcastBridgeCall(endpoint: endpoint, requestJson: nil))
}

private func podcastGlobalCall(
    _ endpoint: String,
    _ request: UnsafePointer<CChar>?
) -> UnsafeMutablePointer<CChar>? {
    guard let request = PodcastBridgeCompat.string(request) else { return nil }
    return PodcastBridgeCompat.cString(
        podcastBridgeGlobalCall(endpoint: endpoint, requestJson: request))
}

func nmp_app_podcast_normalize_feed_url(_ r: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? { podcastGlobalCall("nmp_app_podcast_normalize_feed_url", r) }
func nmp_app_podcast_npub_from_hex(_ r: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? { podcastGlobalCall("nmp_app_podcast_npub_from_hex", r) }
func nmp_app_podcast_parse_pubkey(_ r: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? { podcastGlobalCall("nmp_app_podcast_parse_pubkey", r) }
func nmp_app_podcast_agent_action_policy(_ r: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? { podcastGlobalCall("nmp_app_podcast_agent_action_policy", r) }
func nmp_app_podcast_byok_authorization(_ r: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? { podcastGlobalCall("nmp_app_podcast_byok_authorization", r) }

func nmp_app_podcast_threading_projection(_ h: UnsafeMutableRawPointer?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_threading_projection") }
func nmp_app_podcast_agent_empty_state(_ h: UnsafeMutableRawPointer?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_agent_empty_state") }
func nmp_app_podcast_library_summary(_ h: UnsafeMutableRawPointer?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_library_summary") }
func nmp_app_podcast_library_followed_podcasts(_ h: UnsafeMutableRawPointer?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_library_followed_podcasts") }
func nmp_app_podcast_library_owned_podcasts(_ h: UnsafeMutableRawPointer?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_library_owned_podcasts") }
func nmp_app_podcast_library_download_rows(_ h: UnsafeMutableRawPointer?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_library_download_rows") }
func nmp_app_podcast_library_starred_episodes(_ h: UnsafeMutableRawPointer?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_library_starred_episodes") }
func nmp_app_podcast_library_categorization_prompt(_ h: UnsafeMutableRawPointer?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_library_categorization_prompt") }
func nmp_app_podcast_agent_tts_default_voice(_ h: UnsafeMutableRawPointer?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_agent_tts_default_voice") }
func nmp_app_podcast_agent_generated_podcast_descriptor(_ h: UnsafeMutableRawPointer?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_agent_generated_podcast_descriptor") }
func nmp_app_podcast_now_playing_tool_result(_ h: UnsafeMutableRawPointer?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_now_playing_tool_result") }
func nmp_app_podcast_provider_model_catalog(_ h: UnsafeMutableRawPointer?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_provider_model_catalog") }
func nmp_app_podcast_speech_model_catalog(_ h: UnsafeMutableRawPointer?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_speech_model_catalog") }
func nmp_app_podcast_local_model_catalog(_ h: UnsafeMutableRawPointer?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_local_model_catalog") }
func nmp_app_podcast_validate_openrouter_key(_ h: UnsafeMutableRawPointer?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_validate_openrouter_key") }
func nmp_app_podcast_validate_elevenlabs_key(_ h: UnsafeMutableRawPointer?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_validate_elevenlabs_key") }
func nmp_app_podcast_elevenlabs_voice_catalog(_ h: UnsafeMutableRawPointer?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_elevenlabs_voice_catalog") }

func nmp_app_podcast_audio_report(_ h: UnsafeMutableRawPointer?, _ r: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_audio_report", r) }
func nmp_app_podcast_download_report(_ h: UnsafeMutableRawPointer?, _ r: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_download_report", r) }
func nmp_app_podcast_http_report(_ h: UnsafeMutableRawPointer?, _ r: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_http_report", r) }
func nmp_app_podcast_itunes_directory_search(_ h: UnsafeMutableRawPointer?, _ r: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_itunes_directory_search", r) }
func nmp_app_podcast_itunes_lookup_feed_url(_ h: UnsafeMutableRawPointer?, _ r: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_itunes_lookup_feed_url", r) }
func nmp_app_podcast_itunes_top_podcasts(_ h: UnsafeMutableRawPointer?, _ r: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_itunes_top_podcasts", r) }
func nmp_app_podcast_threading_active_topics(_ h: UnsafeMutableRawPointer?, _ r: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_threading_active_topics", r) }
func nmp_app_podcast_agent_inventory(_ h: UnsafeMutableRawPointer?, _ r: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_agent_inventory", r) }
func nmp_app_podcast_agent_inventory_list(_ h: UnsafeMutableRawPointer?, _ r: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_agent_inventory_list", r) }
func nmp_app_podcast_local_search(_ h: UnsafeMutableRawPointer?, _ r: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_local_search", r) }
func nmp_app_podcast_home_continue_listening(_ h: UnsafeMutableRawPointer?, _ r: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_home_continue_listening", r) }
func nmp_app_podcast_home_triage_rollup(_ h: UnsafeMutableRawPointer?, _ r: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_home_triage_rollup", r) }
func nmp_app_podcast_home_subscription_list(_ h: UnsafeMutableRawPointer?, _ r: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_home_subscription_list", r) }
func nmp_app_podcast_home_category_cards(_ h: UnsafeMutableRawPointer?, _ r: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_home_category_cards", r) }
func nmp_app_podcast_carplay_listen_now(_ h: UnsafeMutableRawPointer?, _ r: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_carplay_listen_now", r) }
func nmp_app_podcast_carplay_shows(_ h: UnsafeMutableRawPointer?, _ r: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_carplay_shows", r) }
func nmp_app_podcast_carplay_show_episodes(_ h: UnsafeMutableRawPointer?, _ r: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_carplay_show_episodes", r) }
func nmp_app_podcast_carplay_downloads(_ h: UnsafeMutableRawPointer?, _ r: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_carplay_downloads", r) }
func nmp_app_podcast_library_show_episodes(_ h: UnsafeMutableRawPointer?, _ r: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_library_show_episodes", r) }
func nmp_app_podcast_library_podcast_stats(_ h: UnsafeMutableRawPointer?, _ r: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_library_podcast_stats", r) }
func nmp_app_podcast_library_episode_for_audio_url(_ h: UnsafeMutableRawPointer?, _ r: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_library_episode_for_audio_url", r) }
func nmp_app_podcast_library_all_episodes(_ h: UnsafeMutableRawPointer?, _ r: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_library_all_episodes", r) }
func nmp_app_podcast_library_all_podcasts(_ h: UnsafeMutableRawPointer?, _ r: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_library_all_podcasts", r) }
func nmp_app_podcast_library_categories(_ h: UnsafeMutableRawPointer?, _ r: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_library_categories", r) }
func nmp_app_podcast_library_episode_lookup(_ h: UnsafeMutableRawPointer?, _ r: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_library_episode_lookup", r) }
func nmp_app_podcast_library_subscription_status(_ h: UnsafeMutableRawPointer?, _ r: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_library_subscription_status", r) }
func nmp_app_podcast_library_podcast_for_owner_pubkey(_ h: UnsafeMutableRawPointer?, _ r: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_library_podcast_for_owner_pubkey", r) }
func nmp_app_podcast_library_categorization_parse(_ h: UnsafeMutableRawPointer?, _ r: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_library_categorization_parse", r) }
func nmp_app_podcast_library_category_change(_ h: UnsafeMutableRawPointer?, _ r: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_library_category_change", r) }
func nmp_app_podcast_agent_chat_title_prompt(_ h: UnsafeMutableRawPointer?, _ r: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_agent_chat_title_prompt", r) }
func nmp_app_podcast_agent_chat_title_parse(_ h: UnsafeMutableRawPointer?, _ r: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_agent_chat_title_parse", r) }
func nmp_app_podcast_agent_nostr_peer_prompt(_ h: UnsafeMutableRawPointer?, _ r: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_agent_nostr_peer_prompt", r) }
func nmp_app_podcast_agent_system_prompt(_ h: UnsafeMutableRawPointer?, _ r: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_agent_system_prompt", r) }
func nmp_app_podcast_agent_conversation_history(_ h: UnsafeMutableRawPointer?, _ r: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_agent_conversation_history", r) }
func nmp_app_podcast_agent_voice_list(_ h: UnsafeMutableRawPointer?, _ r: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_agent_voice_list", r) }
func nmp_app_podcast_agent_youtube_search_plan(_ h: UnsafeMutableRawPointer?, _ r: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_agent_youtube_search_plan", r) }
func nmp_app_podcast_agent_youtube_search_results(_ h: UnsafeMutableRawPointer?, _ r: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_agent_youtube_search_results", r) }
func nmp_app_podcast_agent_directory_search_plan(_ h: UnsafeMutableRawPointer?, _ r: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_agent_directory_search_plan", r) }
func nmp_app_podcast_agent_directory_search_results(_ h: UnsafeMutableRawPointer?, _ r: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_agent_directory_search_results", r) }
func nmp_app_podcast_agent_category_list(_ h: UnsafeMutableRawPointer?, _ r: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_agent_category_list", r) }
func nmp_app_podcast_agent_episode_list_plan(_ h: UnsafeMutableRawPointer?, _ r: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_agent_episode_list_plan", r) }
func nmp_app_podcast_agent_episode_list_results(_ h: UnsafeMutableRawPointer?, _ r: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_agent_episode_list_results", r) }
func nmp_app_podcast_agent_episode_list_error(_ h: UnsafeMutableRawPointer?, _ r: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_agent_episode_list_error", r) }
func nmp_app_podcast_agent_owned_podcast_tool(_ h: UnsafeMutableRawPointer?, _ r: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_agent_owned_podcast_tool", r) }
func nmp_app_podcast_agent_search_tool(_ h: UnsafeMutableRawPointer?, _ r: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_agent_search_tool", r) }
func nmp_app_podcast_agent_action_tool(_ h: UnsafeMutableRawPointer?, _ r: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_agent_action_tool", r) }
func nmp_app_podcast_storage_breakdown(_ h: UnsafeMutableRawPointer?, _ r: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_storage_breakdown", r) }
func nmp_app_podcast_agent_tts_episode_plan(_ h: UnsafeMutableRawPointer?, _ r: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_agent_tts_episode_plan", r) }
func nmp_app_podcast_agent_tts_tool_plan(_ h: UnsafeMutableRawPointer?, _ r: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_agent_tts_tool_plan", r) }
func nmp_app_podcast_agent_tts_tool_result(_ h: UnsafeMutableRawPointer?, _ r: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_agent_tts_tool_result", r) }
func nmp_app_podcast_agent_voice_configure_plan(_ h: UnsafeMutableRawPointer?, _ r: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_agent_voice_configure_plan", r) }
func nmp_app_podcast_agent_voice_configure_result(_ h: UnsafeMutableRawPointer?, _ r: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_agent_voice_configure_result", r) }
func nmp_app_podcast_voice_report(_ h: UnsafeMutableRawPointer?, _ r: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_voice_report", r) }
func nmp_app_podcast_network_report(_ h: UnsafeMutableRawPointer?, _ r: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_network_report", r) }
func nmp_app_podcast_transcript_report(_ h: UnsafeMutableRawPointer?, _ r: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_transcript_report", r) }
func nmp_app_podcast_transcript_ingest_plan(_ h: UnsafeMutableRawPointer?, _ r: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_transcript_ingest_plan", r) }
func nmp_app_podcast_transcript_auto_ingest_candidates(_ h: UnsafeMutableRawPointer?, _ r: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_transcript_auto_ingest_candidates", r) }
func nmp_app_podcast_transcript_tool_result(_ h: UnsafeMutableRawPointer?, _ r: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_transcript_tool_result", r) }
func nmp_app_podcast_episode_mutation_tool_result(_ h: UnsafeMutableRawPointer?, _ r: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_episode_mutation_tool_result", r) }
func nmp_app_podcast_playback_tool_result(_ h: UnsafeMutableRawPointer?, _ r: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_playback_tool_result", r) }
func nmp_app_podcast_external_play_plan(_ h: UnsafeMutableRawPointer?, _ r: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_external_play_plan", r) }
func nmp_app_podcast_agent_ask_enqueue(_ h: UnsafeMutableRawPointer?, _ r: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_agent_ask_enqueue", r) }
func nmp_app_podcast_agent_ask_settle(_ h: UnsafeMutableRawPointer?, _ r: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_agent_ask_settle", r) }
func nmp_app_podcast_memory_remember_text(_ h: UnsafeMutableRawPointer?, _ r: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_memory_remember_text", r) }
func nmp_app_podcast_episode_events(_ h: UnsafeMutableRawPointer?, _ r: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_episode_events", r) }
func nmp_app_podcast_record_episode_event(_ h: UnsafeMutableRawPointer?, _ r: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_record_episode_event", r) }
func nmp_app_podcast_chat_complete(_ h: UnsafeMutableRawPointer?, _ r: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_chat_complete", r) }
func nmp_app_podcast_provider_complete(_ h: UnsafeMutableRawPointer?, _ r: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_provider_complete", r) }
func nmp_app_podcast_provider_embed(_ h: UnsafeMutableRawPointer?, _ r: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_provider_embed", r) }
func nmp_app_podcast_knowledge_query(_ h: UnsafeMutableRawPointer?, _ r: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_knowledge_query", r) }
func nmp_app_podcast_knowledge_similar_episode(_ h: UnsafeMutableRawPointer?, _ r: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_knowledge_similar_episode", r) }
func nmp_app_podcast_knowledge_home_related(_ h: UnsafeMutableRawPointer?, _ r: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_knowledge_home_related", r) }
func nmp_app_podcast_knowledge_resolve_scope(_ h: UnsafeMutableRawPointer?, _ r: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_knowledge_resolve_scope", r) }
func nmp_app_podcast_perplexity_search(_ h: UnsafeMutableRawPointer?, _ r: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_perplexity_search", r) }
func nmp_app_podcast_byok_exchange(_ h: UnsafeMutableRawPointer?, _ r: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_byok_exchange", r) }
func nmp_app_podcast_openrouter_whisper_transcribe(_ h: UnsafeMutableRawPointer?, _ r: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_openrouter_whisper_transcribe", r) }
func nmp_app_podcast_elevenlabs_scribe_transcribe(_ h: UnsafeMutableRawPointer?, _ r: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_elevenlabs_scribe_transcribe", r) }
func nmp_app_podcast_assemblyai_transcribe(_ h: UnsafeMutableRawPointer?, _ r: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_assemblyai_transcribe", r) }
func nmp_app_podcast_elevenlabs_tts_synthesize(_ h: UnsafeMutableRawPointer?, _ r: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_elevenlabs_tts_synthesize", r) }
func nmp_app_podcast_generate_image(_ h: UnsafeMutableRawPointer?, _ r: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_generate_image", r) }
func nmp_app_podcast_rerank(_ h: UnsafeMutableRawPointer?, _ r: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? { podcastCall(h, "nmp_app_podcast_rerank", r) }
