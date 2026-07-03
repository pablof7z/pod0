//! UniFFI bridge router for app-domain endpoints still implemented behind the legacy C symbols.

use std::ffi::{c_char, CStr, CString};
use super::handle::PodcastHandle;
type HandleJsonFn = extern "C" fn(*mut PodcastHandle, *const c_char) -> *mut c_char;
type HandleFn = extern "C" fn(*mut PodcastHandle) -> *mut c_char;
type GlobalJsonFn = extern "C" fn(*const c_char) -> *mut c_char;

pub(crate) fn call_podcast_bridge_endpoint(
    handle: &PodcastHandle,
    endpoint: &str,
    request_json: Option<&str>,
) -> Option<String> {
    let handle = handle as *const PodcastHandle as *mut PodcastHandle;
    match endpoint {
        "nmp_app_podcast_threading_projection" => {
            call_handle(handle, super::nmp_app_podcast_threading_projection)
        }
        "nmp_app_podcast_agent_empty_state" => {
            call_handle(handle, super::nmp_app_podcast_agent_empty_state)
        }
        "nmp_app_podcast_library_summary" => {
            call_handle(handle, super::nmp_app_podcast_library_summary)
        }
        "nmp_app_podcast_library_followed_podcasts" => {
            call_handle(handle, super::nmp_app_podcast_library_followed_podcasts)
        }
        "nmp_app_podcast_library_owned_podcasts" => {
            call_handle(handle, super::nmp_app_podcast_library_owned_podcasts)
        }
        "nmp_app_podcast_library_download_rows" => {
            call_handle(handle, super::nmp_app_podcast_library_download_rows)
        }
        "nmp_app_podcast_library_starred_episodes" => {
            call_handle(handle, super::nmp_app_podcast_library_starred_episodes)
        }
        "nmp_app_podcast_library_categorization_prompt" => {
            call_handle(handle, super::nmp_app_podcast_library_categorization_prompt)
        }
        "nmp_app_podcast_agent_tts_default_voice" => {
            call_handle(handle, super::nmp_app_podcast_agent_tts_default_voice)
        }
        "nmp_app_podcast_agent_generated_podcast_descriptor" => call_handle(
            handle,
            super::nmp_app_podcast_agent_generated_podcast_descriptor,
        ),
        "nmp_app_podcast_now_playing_tool_result" => {
            call_handle(handle, super::nmp_app_podcast_now_playing_tool_result)
        }
        "nmp_app_podcast_provider_model_catalog" => {
            call_handle(handle, super::nmp_app_podcast_provider_model_catalog)
        }
        "nmp_app_podcast_speech_model_catalog" => {
            call_handle(handle, super::nmp_app_podcast_speech_model_catalog)
        }
        "nmp_app_podcast_local_model_catalog" => {
            call_handle(handle, super::nmp_app_podcast_local_model_catalog)
        }
        "nmp_app_podcast_validate_openrouter_key" => {
            call_handle(handle, super::nmp_app_podcast_validate_openrouter_key)
        }
        "nmp_app_podcast_validate_elevenlabs_key" => {
            call_handle(handle, super::nmp_app_podcast_validate_elevenlabs_key)
        }
        "nmp_app_podcast_elevenlabs_voice_catalog" => {
            call_handle(handle, super::nmp_app_podcast_elevenlabs_voice_catalog)
        }
        "nmp_app_podcast_audio_report" => {
            call_handle_json(handle, request_json, super::nmp_app_podcast_audio_report)
        }
        "nmp_app_podcast_download_report" => {
            call_handle_json(handle, request_json, super::nmp_app_podcast_download_report)
        }
        "nmp_app_podcast_http_report" => {
            call_handle_json(handle, request_json, super::nmp_app_podcast_http_report)
        }
        "nmp_app_podcast_itunes_directory_search" => call_handle_json(
            handle,
            request_json,
            super::nmp_app_podcast_itunes_directory_search,
        ),
        "nmp_app_podcast_itunes_lookup_feed_url" => call_handle_json(
            handle,
            request_json,
            super::nmp_app_podcast_itunes_lookup_feed_url,
        ),
        "nmp_app_podcast_itunes_top_podcasts" => call_handle_json(
            handle,
            request_json,
            super::nmp_app_podcast_itunes_top_podcasts,
        ),
        "nmp_app_podcast_threading_active_topics" => call_handle_json(
            handle,
            request_json,
            super::nmp_app_podcast_threading_active_topics,
        ),
        "nmp_app_podcast_agent_inventory" => {
            call_handle_json(handle, request_json, super::nmp_app_podcast_agent_inventory)
        }
        "nmp_app_podcast_agent_inventory_list" => call_handle_json(
            handle,
            request_json,
            super::nmp_app_podcast_agent_inventory_list,
        ),
        "nmp_app_podcast_local_search" => {
            call_handle_json(handle, request_json, super::nmp_app_podcast_local_search)
        }
        "nmp_app_podcast_home_continue_listening" => call_handle_json(
            handle,
            request_json,
            super::nmp_app_podcast_home_continue_listening,
        ),
        "nmp_app_podcast_home_triage_rollup" => call_handle_json(
            handle,
            request_json,
            super::nmp_app_podcast_home_triage_rollup,
        ),
        "nmp_app_podcast_home_subscription_list" => call_handle_json(
            handle,
            request_json,
            super::nmp_app_podcast_home_subscription_list,
        ),
        "nmp_app_podcast_home_category_cards" => call_handle_json(
            handle,
            request_json,
            super::nmp_app_podcast_home_category_cards,
        ),
        "nmp_app_podcast_carplay_listen_now" => call_handle_json(
            handle,
            request_json,
            super::nmp_app_podcast_carplay_listen_now,
        ),
        "nmp_app_podcast_carplay_shows" => {
            call_handle_json(handle, request_json, super::nmp_app_podcast_carplay_shows)
        }
        "nmp_app_podcast_carplay_show_episodes" => call_handle_json(
            handle,
            request_json,
            super::nmp_app_podcast_carplay_show_episodes,
        ),
        "nmp_app_podcast_carplay_downloads" => call_handle_json(
            handle,
            request_json,
            super::nmp_app_podcast_carplay_downloads,
        ),
        "nmp_app_podcast_library_show_episodes" => call_handle_json(
            handle,
            request_json,
            super::nmp_app_podcast_library_show_episodes,
        ),
        "nmp_app_podcast_library_podcast_stats" => call_handle_json(
            handle,
            request_json,
            super::nmp_app_podcast_library_podcast_stats,
        ),
        "nmp_app_podcast_library_episode_for_audio_url" => call_handle_json(
            handle,
            request_json,
            super::nmp_app_podcast_library_episode_for_audio_url,
        ),
        "nmp_app_podcast_library_all_episodes" => call_handle_json(
            handle,
            request_json,
            super::nmp_app_podcast_library_all_episodes,
        ),
        "nmp_app_podcast_library_all_podcasts" => call_handle_json(
            handle,
            request_json,
            super::nmp_app_podcast_library_all_podcasts,
        ),
        "nmp_app_podcast_library_categories" => call_handle_json(
            handle,
            request_json,
            super::nmp_app_podcast_library_categories,
        ),
        "nmp_app_podcast_library_episode_lookup" => call_handle_json(
            handle,
            request_json,
            super::nmp_app_podcast_library_episode_lookup,
        ),
        "nmp_app_podcast_library_subscription_status" => call_handle_json(
            handle,
            request_json,
            super::nmp_app_podcast_library_subscription_status,
        ),
        "nmp_app_podcast_library_podcast_for_owner_pubkey" => call_handle_json(
            handle,
            request_json,
            super::nmp_app_podcast_library_podcast_for_owner_pubkey,
        ),
        "nmp_app_podcast_library_categorization_parse" => call_handle_json(
            handle,
            request_json,
            super::nmp_app_podcast_library_categorization_parse,
        ),
        "nmp_app_podcast_library_category_change" => call_handle_json(
            handle,
            request_json,
            super::nmp_app_podcast_library_category_change,
        ),
        "nmp_app_podcast_agent_chat_title_prompt" => call_handle_json(
            handle,
            request_json,
            super::nmp_app_podcast_agent_chat_title_prompt,
        ),
        "nmp_app_podcast_agent_chat_title_parse" => call_handle_json(
            handle,
            request_json,
            super::nmp_app_podcast_agent_chat_title_parse,
        ),
        "nmp_app_podcast_agent_nostr_peer_prompt" => call_handle_json(
            handle,
            request_json,
            super::nmp_app_podcast_agent_nostr_peer_prompt,
        ),
        "nmp_app_podcast_agent_system_prompt" => call_handle_json(
            handle,
            request_json,
            super::nmp_app_podcast_agent_system_prompt,
        ),
        "nmp_app_podcast_agent_conversation_history" => call_handle_json(
            handle,
            request_json,
            super::nmp_app_podcast_agent_conversation_history,
        ),
        "nmp_app_podcast_agent_voice_list" => call_handle_json(
            handle,
            request_json,
            super::nmp_app_podcast_agent_voice_list,
        ),
        "nmp_app_podcast_agent_youtube_search_plan" => call_handle_json(
            handle,
            request_json,
            super::nmp_app_podcast_agent_youtube_search_plan,
        ),
        "nmp_app_podcast_agent_youtube_search_results" => call_handle_json(
            handle,
            request_json,
            super::nmp_app_podcast_agent_youtube_search_results,
        ),
        "nmp_app_podcast_agent_directory_search_plan" => call_handle_json(
            handle,
            request_json,
            super::nmp_app_podcast_agent_directory_search_plan,
        ),
        "nmp_app_podcast_agent_directory_search_results" => call_handle_json(
            handle,
            request_json,
            super::nmp_app_podcast_agent_directory_search_results,
        ),
        "nmp_app_podcast_agent_category_list" => call_handle_json(
            handle,
            request_json,
            super::nmp_app_podcast_agent_category_list,
        ),
        "nmp_app_podcast_agent_episode_list_plan" => call_handle_json(
            handle,
            request_json,
            super::nmp_app_podcast_agent_episode_list_plan,
        ),
        "nmp_app_podcast_agent_episode_list_results" => call_handle_json(
            handle,
            request_json,
            super::nmp_app_podcast_agent_episode_list_results,
        ),
        "nmp_app_podcast_agent_episode_list_error" => call_handle_json(
            handle,
            request_json,
            super::nmp_app_podcast_agent_episode_list_error,
        ),
        "nmp_app_podcast_agent_owned_podcast_tool" => call_handle_json(
            handle,
            request_json,
            super::nmp_app_podcast_agent_owned_podcast_tool,
        ),
        "nmp_app_podcast_agent_search_tool" => call_handle_json(
            handle,
            request_json,
            super::nmp_app_podcast_agent_search_tool,
        ),
        "nmp_app_podcast_agent_action_tool" => call_handle_json(
            handle,
            request_json,
            super::nmp_app_podcast_agent_action_tool,
        ),
        "nmp_app_podcast_storage_breakdown" => call_handle_json(
            handle,
            request_json,
            super::nmp_app_podcast_storage_breakdown,
        ),
        "nmp_app_podcast_agent_tts_episode_plan" => call_handle_json(
            handle,
            request_json,
            super::nmp_app_podcast_agent_tts_episode_plan,
        ),
        "nmp_app_podcast_agent_tts_tool_plan" => call_handle_json(
            handle,
            request_json,
            super::nmp_app_podcast_agent_tts_tool_plan,
        ),
        "nmp_app_podcast_agent_tts_tool_result" => call_handle_json(
            handle,
            request_json,
            super::nmp_app_podcast_agent_tts_tool_result,
        ),
        "nmp_app_podcast_agent_voice_configure_plan" => call_handle_json(
            handle,
            request_json,
            super::nmp_app_podcast_agent_voice_configure_plan,
        ),
        "nmp_app_podcast_agent_voice_configure_result" => call_handle_json(
            handle,
            request_json,
            super::nmp_app_podcast_agent_voice_configure_result,
        ),
        "nmp_app_podcast_voice_report" => {
            call_handle_json(handle, request_json, super::nmp_app_podcast_voice_report)
        }
        "nmp_app_podcast_network_report" => {
            call_handle_json(handle, request_json, super::nmp_app_podcast_network_report)
        }
        "nmp_app_podcast_transcript_report" => call_handle_json(
            handle,
            request_json,
            super::nmp_app_podcast_transcript_report,
        ),
        "nmp_app_podcast_transcript_ingest_plan" => call_handle_json(
            handle,
            request_json,
            super::nmp_app_podcast_transcript_ingest_plan,
        ),
        "nmp_app_podcast_transcript_auto_ingest_candidates" => call_handle_json(
            handle,
            request_json,
            super::nmp_app_podcast_transcript_auto_ingest_candidates,
        ),
        "nmp_app_podcast_transcript_tool_result" => call_handle_json(
            handle,
            request_json,
            super::nmp_app_podcast_transcript_tool_result,
        ),
        "nmp_app_podcast_episode_mutation_tool_result" => call_handle_json(
            handle,
            request_json,
            super::nmp_app_podcast_episode_mutation_tool_result,
        ),
        "nmp_app_podcast_playback_tool_result" => call_handle_json(
            handle,
            request_json,
            super::nmp_app_podcast_playback_tool_result,
        ),
        "nmp_app_podcast_external_play_plan" => call_handle_json(
            handle,
            request_json,
            super::nmp_app_podcast_external_play_plan,
        ),
        "nmp_app_podcast_agent_ask_enqueue" => call_handle_json(
            handle,
            request_json,
            super::nmp_app_podcast_agent_ask_enqueue,
        ),
        "nmp_app_podcast_agent_ask_settle" => call_handle_json(
            handle,
            request_json,
            super::nmp_app_podcast_agent_ask_settle,
        ),
        "nmp_app_podcast_memory_remember_text" => call_handle_json(
            handle,
            request_json,
            super::nmp_app_podcast_memory_remember_text,
        ),
        "nmp_app_podcast_episode_events" => {
            call_handle_json(handle, request_json, super::nmp_app_podcast_episode_events)
        }
        "nmp_app_podcast_record_episode_event" => call_handle_json(
            handle,
            request_json,
            super::nmp_app_podcast_record_episode_event,
        ),
        "nmp_app_podcast_chat_complete" => {
            call_handle_json(handle, request_json, super::nmp_app_podcast_chat_complete)
        }
        "nmp_app_podcast_provider_complete" => call_handle_json(
            handle,
            request_json,
            super::nmp_app_podcast_provider_complete,
        ),
        "nmp_app_podcast_provider_embed" => {
            call_handle_json(handle, request_json, super::nmp_app_podcast_provider_embed)
        }
        "nmp_app_podcast_knowledge_query" => {
            call_handle_json(handle, request_json, super::nmp_app_podcast_knowledge_query)
        }
        "nmp_app_podcast_knowledge_similar_episode" => call_handle_json(
            handle,
            request_json,
            super::nmp_app_podcast_knowledge_similar_episode,
        ),
        "nmp_app_podcast_knowledge_home_related" => call_handle_json(
            handle,
            request_json,
            super::nmp_app_podcast_knowledge_home_related,
        ),
        "nmp_app_podcast_knowledge_chunk" => {
            call_handle_json(handle, request_json, super::nmp_app_podcast_knowledge_chunk)
        }
        "nmp_app_podcast_knowledge_resolve_scope" => call_handle_json(
            handle,
            request_json,
            super::nmp_app_podcast_knowledge_resolve_scope,
        ),
        "nmp_app_podcast_perplexity_search" => call_handle_json(
            handle,
            request_json,
            super::nmp_app_podcast_perplexity_search,
        ),
        "nmp_app_podcast_byok_exchange" => {
            call_handle_json(handle, request_json, super::nmp_app_podcast_byok_exchange)
        }
        "nmp_app_podcast_openrouter_whisper_transcribe" => call_handle_json(
            handle,
            request_json,
            super::nmp_app_podcast_openrouter_whisper_transcribe,
        ),
        "nmp_app_podcast_elevenlabs_scribe_transcribe" => call_handle_json(
            handle,
            request_json,
            super::nmp_app_podcast_elevenlabs_scribe_transcribe,
        ),
        "nmp_app_podcast_assemblyai_transcribe" => call_handle_json(
            handle,
            request_json,
            super::nmp_app_podcast_assemblyai_transcribe,
        ),
        "nmp_app_podcast_elevenlabs_tts_synthesize" => call_handle_json(
            handle,
            request_json,
            super::nmp_app_podcast_elevenlabs_tts_synthesize,
        ),
        "nmp_app_podcast_generate_image" => {
            call_handle_json(handle, request_json, super::nmp_app_podcast_generate_image)
        }
        "nmp_app_podcast_rerank" => {
            call_handle_json(handle, request_json, super::nmp_app_podcast_rerank)
        }
        _ => None,
    }
}
pub(crate) fn call_podcast_global_endpoint(endpoint: &str, request_json: &str) -> Option<String> {
    match endpoint {
        "nmp_app_podcast_normalize_feed_url" => {
            call_global_json(request_json, super::nmp_app_podcast_normalize_feed_url)
        }
        "nmp_app_podcast_npub_from_hex" => {
            call_global_json(request_json, super::nmp_app_podcast_npub_from_hex)
        }
        "nmp_app_podcast_parse_pubkey" => {
            call_global_json(request_json, super::nmp_app_podcast_parse_pubkey)
        }
        "nmp_app_podcast_agent_action_policy" => {
            call_global_json(request_json, super::nmp_app_podcast_agent_action_policy)
        }
        "nmp_app_podcast_byok_authorization" => {
            call_global_json(request_json, super::nmp_app_podcast_byok_authorization)
        }
        _ => None,
    }
}

fn call_handle_json(
    handle: *mut PodcastHandle,
    request_json: Option<&str>,
    func: HandleJsonFn,
) -> Option<String> {
    let request_json = request_json?;
    let request = CString::new(request_json).ok()?;
    take_c_string(func(handle, request.as_ptr()))
}

fn call_handle(handle: *mut PodcastHandle, func: HandleFn) -> Option<String> {
    take_c_string(func(handle))
}

fn call_global_json(request_json: &str, func: GlobalJsonFn) -> Option<String> {
    let request = CString::new(request_json).ok()?;
    take_c_string(func(request.as_ptr()))
}

fn take_c_string(ptr: *mut c_char) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    let value = unsafe { CStr::from_ptr(ptr) }
        .to_string_lossy()
        .into_owned();
    unsafe {
        drop(CString::from_raw(ptr));
    }
    Some(value)
}
