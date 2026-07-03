//! Podcast per-app FFI surface.
//!
//! App-domain `extern "C"` symbols still linked by native shells:
//!
//! - [`nmp_app_podcast_register`] — install the explicit NMP substrate and
//!   protocol modules into the supplied `NmpApp`, then return an opaque handle
//!   for subsequent snapshot / unregister calls.
//! - [`nmp_app_podcast_snapshot`] — serialize the current app state into a
//!   freshly-allocated nul-terminated JSON C string. Swift owns the pointer
//!   until it calls `nmp_app_podcast_snapshot_free`.
//! - [`nmp_app_podcast_snapshot_free`] — companion deallocator for the
//!   snapshot string.
//! - [`nmp_app_podcast_unregister`] — drop the handle and free associated
//!   resources. Idempotent.
//!
//! ## Doctrine
//!
//! * **D0** — `nmp-core` never carries podcast-domain nouns; this crate is
//!   the composition point.
//! * **D6** — every entry point is fire-and-forget. Null pointers, missing
//!   strings, serialization failures, and poisoned mutexes all degrade
//!   silently rather than raising across the FFI.
//! * **No business logic in native shells** — shells take the JSON string,
//!   decode to the appropriate types, and render. All logic happens in Rust.
//!
//! ## Module layout
//!
//! Split across sub-modules to keep each file under the 500-LOC hard ceiling.
//! Every remaining app-domain `pub extern "C"` symbol is re-exported below.

pub mod actions;
mod agent_action_tool;
mod agent_ask;
mod agent_category_list;
mod agent_chat_title;
pub(crate) mod agent_context;
mod agent_conversation_history;
mod agent_directory_search;
mod agent_empty_state;
mod agent_episode_list;
mod agent_inventory;
mod agent_inventory_list;
mod agent_nostr_peer_prompt;
mod agent_owned_podcast_tool;
mod agent_search_tool;
mod agent_system_prompt;
mod agent_tts_plan;
mod agent_tts_tool;
mod agent_voice_list;
mod agent_youtube_search;
mod assemblyai_transcript;
mod audio_report;
mod byok_auth;
mod carplay_projection;
mod chat_complete;
mod data_dir;
pub mod dispatch_action;
mod download_report;
mod elevenlabs_scribe;
mod elevenlabs_tts;
mod elevenlabs_voice_catalog;
mod episode_events;
mod episode_mutation_tool_result;
mod external_play_plan;
mod feed_url_normalizer;
pub(crate) mod guard;
pub(crate) mod handle;
mod helpers;
mod home_category_projection;
mod home_projection;
mod http_report;
mod identity_format;
mod image_generation;
mod itunes_directory;
mod knowledge_query;
mod knowledge_scope;
mod library_categorization;
mod library_category_change;
mod library_projection;
mod local_llm;
mod local_model_catalog;
mod local_search;
mod memory_remember_text;
mod network_report;
mod openrouter_whisper;
mod owned_podcast_lookup;
mod perplexity_search;
#[cfg(test)]
mod platform_bridge_tests;
mod playback_tool_result;
pub mod projections;
#[cfg(test)]
mod projections_tests;
#[cfg(test)]
mod projections_tests_ext;
mod provider_complete;
mod provider_embeddings;
mod provider_key_validation;
mod provider_model_catalog;
mod register;
mod register_observers;
pub(crate) mod relay_persist;
mod rerank;
mod runtime_facade;
pub(crate) mod snapshot;
mod snapshot_categories;
mod snapshot_domain_builders;
pub(crate) mod snapshot_domain_projections;
mod snapshot_domain_store_helpers;
mod snapshot_downloads;
#[cfg(test)]
mod snapshot_golden_tests;
mod snapshot_identity;
mod snapshot_library;
mod snapshot_owned;
mod snapshot_queue;
mod snapshot_relays;
mod snapshot_settings;
#[cfg(test)]
mod snapshot_tests;
#[cfg(test)]
mod snapshot_tests_ext;
mod snapshot_update;
mod snapshot_widget;
mod speech_model_catalog;
mod storage_projection;
mod threading_projection;
mod transcript_plan;
mod transcript_report;
mod transcript_tool_result;
mod uniffi_bridge_calls;
pub mod uniffi_facade;
mod voice_report;

pub use actions::{
    AgentActionModule, AgentApproveAction, AgentChatAction, AgentClearConversationAction,
    AgentDenyAction, AgentPicksModule, AgentTaskIntent, AgentTasksAction, AgentTasksModule,
    CancelAllDownloadsAction, CancelDownloadAction, CategorizationAction, CategorizationModule,
    ChaptersAction, ChaptersActionModule, ClipAction, ClipActionModule, DownloadEpisodeAction,
    InboxAction, InboxActionModule, KnowledgeAction, KnowledgeActionModule, MemoryAction,
    MemoryActionModule, NipF4PublishModule, PauseAction, PauseDownloadAction, PicksAction,
    PlayAction, PlayerAction, PlayerActionModule, PodcastAction, PodcastActionModule,
    PublishAction, QueueAction, QueueActionModule, ResumeDownloadAction, SeekAction,
    SendAgentMessageAction, SetSleepTimerAction, SetSpeedAction, SetVoiceAction, SetVolumeAction,
    SettingsAction, SettingsActionModule, SiriAction, SiriActionModule, SiriPlayLatestAction,
    SiriResumeAction, SpeakAction, StopAction, StopVoiceAction, VoiceAction, VoiceActionModule,
    ACTION_AGENT_APPROVE, ACTION_AGENT_CLEAR, ACTION_AGENT_DENY, ACTION_AGENT_SEND,
    ACTION_CLIP_AUTO_SNIP, ACTION_CLIP_CREATE, ACTION_CLIP_DELETE, ACTION_INBOX_DISMISS,
    ACTION_INBOX_MARK_LISTENED, ACTION_INBOX_TRIAGE, ACTION_KNOWLEDGE_CLEAR_RESULTS,
    ACTION_KNOWLEDGE_INDEX_EPISODE, ACTION_KNOWLEDGE_SEARCH, ACTION_PLAYER_CANCEL_ALL_DOWNLOADS,
    ACTION_PLAYER_CANCEL_DOWNLOAD, ACTION_PLAYER_DOWNLOAD, ACTION_PLAYER_PAUSE,
    ACTION_PLAYER_PAUSE_DOWNLOAD, ACTION_PLAYER_PLAY, ACTION_PLAYER_RESUME_DOWNLOAD,
    ACTION_PLAYER_SEEK, ACTION_PLAYER_SET_SLEEP_TIMER, ACTION_PLAYER_SET_SPEED,
    ACTION_PLAYER_SET_VOLUME, ACTION_PLAYER_SKIP_BACKWARD, ACTION_PLAYER_SKIP_FORWARD,
    ACTION_PLAYER_STOP, ACTION_PUBLISH_CREATE_OWNED, ACTION_PUBLISH_PUBLISH_AUTHOR_CLAIM,
    ACTION_PUBLISH_PUBLISH_EPISODE, ACTION_PUBLISH_PUBLISH_SHOW, ACTION_PUBLISH_REMOVE_OWNED,
    ACTION_SIRI_PLAY_LATEST, ACTION_SIRI_RESUME, ACTION_VOICE_ACTIVATE, ACTION_VOICE_DEACTIVATE,
    ACTION_VOICE_SET_VOICE, ACTION_VOICE_SPEAK, ACTION_VOICE_STOP, PICKS_LIMIT, PICKS_PER_SHOW_CAP,
};
pub use agent_action_tool::{
    nmp_app_podcast_agent_action_policy, nmp_app_podcast_agent_action_tool,
};
pub use agent_ask::{
    nmp_app_podcast_agent_ask_enqueue, nmp_app_podcast_agent_ask_settle,
};
pub use agent_category_list::nmp_app_podcast_agent_category_list;
pub use agent_chat_title::{
    nmp_app_podcast_agent_chat_title_parse, nmp_app_podcast_agent_chat_title_prompt,
};
pub use agent_conversation_history::nmp_app_podcast_agent_conversation_history;
pub use agent_directory_search::{
    nmp_app_podcast_agent_directory_search_plan, nmp_app_podcast_agent_directory_search_results,
};
pub use agent_empty_state::nmp_app_podcast_agent_empty_state;
pub use agent_episode_list::{
    nmp_app_podcast_agent_episode_list_error, nmp_app_podcast_agent_episode_list_plan,
    nmp_app_podcast_agent_episode_list_results,
};
pub use agent_inventory::nmp_app_podcast_agent_inventory;
pub use agent_inventory_list::nmp_app_podcast_agent_inventory_list;
pub use agent_nostr_peer_prompt::nmp_app_podcast_agent_nostr_peer_prompt;
pub use agent_owned_podcast_tool::nmp_app_podcast_agent_owned_podcast_tool;
pub use agent_search_tool::nmp_app_podcast_agent_search_tool;
pub use agent_system_prompt::nmp_app_podcast_agent_system_prompt;
pub use agent_tts_plan::{
    nmp_app_podcast_agent_generated_podcast_descriptor, nmp_app_podcast_agent_tts_default_voice,
    nmp_app_podcast_agent_tts_episode_plan,
};
pub use agent_tts_tool::{
    nmp_app_podcast_agent_tts_tool_plan, nmp_app_podcast_agent_tts_tool_result,
    nmp_app_podcast_agent_voice_configure_plan, nmp_app_podcast_agent_voice_configure_result,
};
pub use agent_voice_list::nmp_app_podcast_agent_voice_list;
pub use agent_youtube_search::{
    nmp_app_podcast_agent_youtube_search_plan, nmp_app_podcast_agent_youtube_search_results,
};
pub use assemblyai_transcript::nmp_app_podcast_assemblyai_transcribe;
pub use audio_report::nmp_app_podcast_audio_report;
pub use byok_auth::{nmp_app_podcast_byok_authorization, nmp_app_podcast_byok_exchange};
pub use carplay_projection::{
    nmp_app_podcast_carplay_downloads, nmp_app_podcast_carplay_listen_now,
    nmp_app_podcast_carplay_show_episodes, nmp_app_podcast_carplay_shows,
};
pub use chat_complete::nmp_app_podcast_chat_complete;
pub use data_dir::nmp_app_podcast_set_data_dir;
pub use dispatch_action::nmp_app_podcast_dispatch_action;
pub use download_report::nmp_app_podcast_download_report;
pub use elevenlabs_scribe::nmp_app_podcast_elevenlabs_scribe_transcribe;
pub use elevenlabs_tts::nmp_app_podcast_elevenlabs_tts_synthesize;
pub use elevenlabs_voice_catalog::nmp_app_podcast_elevenlabs_voice_catalog;
pub use episode_events::{nmp_app_podcast_episode_events, nmp_app_podcast_record_episode_event};
pub use episode_mutation_tool_result::nmp_app_podcast_episode_mutation_tool_result;
pub use external_play_plan::nmp_app_podcast_external_play_plan;
pub use feed_url_normalizer::nmp_app_podcast_normalize_feed_url;
pub use handle::PodcastHandle;
pub use home_category_projection::nmp_app_podcast_home_category_cards;
pub use home_projection::{
    nmp_app_podcast_home_continue_listening, nmp_app_podcast_home_subscription_list,
    nmp_app_podcast_home_triage_rollup,
};
pub use http_report::nmp_app_podcast_http_report;
pub use identity_format::{nmp_app_podcast_npub_from_hex, nmp_app_podcast_parse_pubkey};
pub use image_generation::nmp_app_podcast_generate_image;
pub use itunes_directory::{
    nmp_app_podcast_itunes_directory_search, nmp_app_podcast_itunes_lookup_feed_url,
    nmp_app_podcast_itunes_top_podcasts,
};
pub use knowledge_query::{
    nmp_app_podcast_knowledge_chunk, nmp_app_podcast_knowledge_home_related,
    nmp_app_podcast_knowledge_query, nmp_app_podcast_knowledge_similar_episode,
};
pub use knowledge_scope::nmp_app_podcast_knowledge_resolve_scope;
pub use library_categorization::{
    nmp_app_podcast_library_categorization_parse, nmp_app_podcast_library_categorization_prompt,
};
pub use library_category_change::nmp_app_podcast_library_category_change;
pub use library_projection::{
    nmp_app_podcast_library_all_episodes, nmp_app_podcast_library_all_podcasts,
    nmp_app_podcast_library_categories, nmp_app_podcast_library_download_rows,
    nmp_app_podcast_library_episode_for_audio_url, nmp_app_podcast_library_episode_lookup,
    nmp_app_podcast_library_followed_podcasts, nmp_app_podcast_library_owned_podcasts,
    nmp_app_podcast_library_podcast_stats, nmp_app_podcast_library_show_episodes,
    nmp_app_podcast_library_starred_episodes, nmp_app_podcast_library_subscription_status,
    nmp_app_podcast_library_summary,
};
pub use local_llm::{nmp_app_clear_local_llm, nmp_app_register_local_llm};
pub use local_model_catalog::nmp_app_podcast_local_model_catalog;
pub use local_search::nmp_app_podcast_local_search;
pub use memory_remember_text::nmp_app_podcast_memory_remember_text;
pub use network_report::nmp_app_podcast_network_report;
pub use openrouter_whisper::nmp_app_podcast_openrouter_whisper_transcribe;
pub use owned_podcast_lookup::nmp_app_podcast_library_podcast_for_owner_pubkey;
pub use perplexity_search::nmp_app_podcast_perplexity_search;
pub use playback_tool_result::{
    nmp_app_podcast_now_playing_tool_result, nmp_app_podcast_playback_tool_result,
};
pub use projections::{
    AccountSummary, AgentMessageSummary, AgentPickSummary, AgentSnapshot, AgentTaskSummary,
    CategoryBrowseItem, ChapterSummary, ClipSummary, CommentSummary, ContactSummary,
    ConversationsSnapshot, DownloadItemSnapshot, DownloadQueueSnapshot, EpisodeSummary, InboxItem,
    KnowledgeSearchResult, MemoryFact, NostrShowSummary, OwnedPodcastInfo, PendingApprovalSnapshot,
    PodcastSummary, SettingsSnapshot, SocialSnapshot, TranscriptEntry, VoiceState, WidgetSnapshot,
};
pub use provider_complete::nmp_app_podcast_provider_complete;
pub use provider_embeddings::nmp_app_podcast_provider_embed;
pub use provider_key_validation::{
    nmp_app_podcast_validate_elevenlabs_key, nmp_app_podcast_validate_openrouter_key,
};
pub use provider_model_catalog::nmp_app_podcast_provider_model_catalog;
pub use register::nmp_app_podcast_register;
pub use rerank::nmp_app_podcast_rerank;
pub use runtime_facade::{
    classify_input_intent_json, decode_nip21_uri_json, dispatch_input_intent_json, nmp_free_string,
};
pub use snapshot::{
    nmp_app_podcast_snapshot, nmp_app_podcast_snapshot_free, nmp_app_podcast_snapshot_rev,
    nmp_app_podcast_unregister, AppRelayRow, PodcastUpdate,
};
pub use speech_model_catalog::nmp_app_podcast_speech_model_catalog;
pub use storage_projection::nmp_app_podcast_storage_breakdown;
pub use threading_projection::{
    nmp_app_podcast_threading_active_topics, nmp_app_podcast_threading_projection,
};
pub use transcript_plan::{
    nmp_app_podcast_transcript_auto_ingest_candidates, nmp_app_podcast_transcript_ingest_plan,
};
pub use transcript_report::nmp_app_podcast_transcript_report;
pub use transcript_tool_result::nmp_app_podcast_transcript_tool_result;
pub use uniffi_facade::{
    PodcastAgentAskSink, PodcastApp, PodcastCapabilitySink, PodcastDispatchOutcome,
    PodcastEventShape, PodcastProfileShape, PodcastRefLiveness, PodcastRefNamespace,
    PodcastRefShape, PodcastUpdateSink,
};
pub use voice_report::nmp_app_podcast_voice_report;
