//! `nmp-app-podcast` — Podcast per-app glue.
//!
//! Composes `nmp-core` (the kernel substrate + event observer slot) with
//! `nmp-defaults` (the canonical NMP composition root) to surface podcast
//! state over a static-lib FFI for the iOS shell.
//!
//! ## Wiring
//!
//! The iOS shell links this one aggregate static library for Podcast. Keeping
//! `nmp-core`, the NIP-46 signer broker, and the Podcast projection in one
//! Rust archive gives the process exactly one copy of `nmp-core` static state.
//!
//! The shell calls `nmp_signer_broker_init` (from `nmp-ffi`) once after `nmp_app_new`, then
//! calls [`ffi::nmp_app_podcast_register`]. The registration:
//!
//! 1. Wires the canonical NMP defaults via `nmp_defaults::register_defaults`.
//! 2. Returns an opaque handle for later snapshots / unregister.
//!
//! On each render tick the shell calls [`ffi::nmp_app_podcast_snapshot`],
//! decodes the JSON, and renders the current podcast state.
//!
//! ## Doctrine
//!
//! * **D0** — kernel emits, this crate composes. No business logic in Swift;
//!   podcast-domain nouns (Episode, Feed, Chapter) live in this crate or in
//!   future `nmp-nip-*` podcast protocol crates, never in `nmp-core`.
//! * **D6** — every FFI symbol degrades silently on null pointers, lock
//!   poisoning, or serialization failure.
//! * **D7** — capabilities report, never decide. The contracts in
//!   [`capability`] are the request/event vocabularies; decision-making
//!   (sleep-timer expiry, end-of-episode policy, retry behaviour) lives in
//!   per-projection actors under [`player`] et al.

pub(crate) mod ad_skip_handler;
pub mod agent_handler;
pub(crate) mod agent_llm;
pub(crate) mod agent_note_handler;
pub(crate) mod agent_note_responder;
pub(crate) mod agent_tools;
pub(crate) mod ai_chapters;
pub(crate) mod ai_chapters_llm;
pub mod capability;
pub(crate) mod categorization;
pub(crate) mod categorization_llm;
pub(crate) mod chapter;
pub(crate) mod clip_handler;
pub(crate) mod clip_boundaries;
pub(crate) mod comments_anchor;
pub(crate) mod comments_handler;
pub(crate) mod discover_nostr;
pub mod download;
pub(crate) mod episode_summary;
pub(crate) mod episode_summary_llm;
pub(crate) mod feed_fetch;
pub mod ffi;
pub(crate) mod host_op_handler;
pub(crate) mod host_op_handler_helpers;
pub(crate) mod host_op_handler_queue;
pub(crate) mod host_op_publish;
pub(crate) mod host_op_publish_lifecycle;
pub(crate) mod identity_handler;
pub(crate) mod inbox_handler;
pub(crate) mod inbox_llm;
pub(crate) mod itunes;
pub mod knowledge;
pub(crate) mod knowledge_fusion;
pub mod llm;
pub mod state;
pub(crate) mod memory_handler;
pub(crate) mod nmp_dispatch;
pub(crate) mod nostr_episodes;
pub(crate) mod picks_handler;
pub(crate) mod picks_llm;
pub mod player;
pub mod queue;
pub(crate) mod snapshot_signal;
pub(crate) mod social_handler;
pub(crate) mod social_publish_handler;
pub mod store;
pub(crate) mod tasks_handler;
pub(crate) mod tasks_schedule;
pub(crate) mod transcript;
pub(crate) mod voice_conversation;
pub(crate) mod voice_handler;

// M2.F — Android JNI shim. Gated `target_os = "android"` so iOS/macOS builds
// remain unaffected. The shim exports `Java_io_f7z_podcast_KernelBridge_*`
// symbols cargo-ndk packs into `libnmp_app_podcast.so`. Same crate, same logic.
#[cfg(target_os = "android")]
pub mod android;

pub use capability::{
    AudioCommand, AudioReport, DownloadCommand, DownloadReport, AUDIO_CAPABILITY_NAMESPACE,
    DOWNLOAD_CAPABILITY_NAMESPACE,
};
pub use download::{DownloadItem, DownloadItemState, DownloadQueue, DEFAULT_MAX_CONCURRENT};
pub use ffi::{
    nmp_app_podcast_agent_action_policy, nmp_app_podcast_agent_action_tool,
    nmp_app_podcast_agent_category_list,
    nmp_app_podcast_agent_chat_title_parse, nmp_app_podcast_agent_chat_title_prompt,
    nmp_app_podcast_agent_conversation_history, nmp_app_podcast_agent_empty_state,
    nmp_app_podcast_agent_directory_search_plan,
    nmp_app_podcast_agent_directory_search_results,
    nmp_app_podcast_agent_episode_list_error, nmp_app_podcast_agent_episode_list_plan,
    nmp_app_podcast_agent_episode_list_results,
    nmp_app_podcast_agent_generated_podcast_descriptor, nmp_app_podcast_agent_inventory,
    nmp_app_podcast_agent_inventory_list, nmp_app_podcast_agent_nostr_peer_prompt,
    nmp_app_podcast_agent_owned_podcast_tool, nmp_app_podcast_agent_search_tool,
    nmp_app_podcast_agent_system_prompt, nmp_app_podcast_agent_tts_default_voice,
    nmp_app_podcast_agent_tts_episode_plan, nmp_app_podcast_agent_tts_tool_plan,
    nmp_app_podcast_agent_tts_tool_result, nmp_app_podcast_agent_voice_configure_plan,
    nmp_app_podcast_agent_voice_configure_result, nmp_app_podcast_agent_voice_list,
    nmp_app_podcast_agent_youtube_search_plan, nmp_app_podcast_agent_youtube_search_results,
    nmp_app_podcast_assemblyai_transcribe, nmp_app_podcast_audio_report, nmp_app_podcast_byok_authorization,
    nmp_app_podcast_byok_exchange, nmp_app_podcast_carplay_downloads,
    nmp_app_podcast_carplay_listen_now, nmp_app_podcast_carplay_show_episodes,
    nmp_app_podcast_carplay_shows, nmp_app_podcast_download_report,
    nmp_app_podcast_elevenlabs_scribe_transcribe,
    nmp_app_podcast_elevenlabs_tts_synthesize, nmp_app_podcast_elevenlabs_voice_catalog,
    nmp_app_podcast_episode_events, nmp_app_podcast_generate_image,
    nmp_app_podcast_normalize_feed_url,
    nmp_app_podcast_home_category_cards,
    nmp_app_podcast_home_continue_listening, nmp_app_podcast_home_subscription_list,
    nmp_app_podcast_home_triage_rollup, nmp_app_podcast_http_report,
    nmp_app_podcast_npub_from_hex, nmp_app_podcast_parse_pubkey,
    nmp_app_podcast_knowledge_home_related,
    nmp_app_podcast_knowledge_resolve_scope,
    nmp_app_podcast_itunes_directory_search, nmp_app_podcast_itunes_lookup_feed_url,
    nmp_app_podcast_itunes_top_podcasts, nmp_app_podcast_library_all_episodes,
    nmp_app_podcast_library_all_podcasts, nmp_app_podcast_library_categories,
    nmp_app_podcast_library_categorization_parse,
    nmp_app_podcast_library_categorization_prompt,
    nmp_app_podcast_library_category_change,
    nmp_app_podcast_library_download_rows, nmp_app_podcast_library_episode_for_audio_url,
    nmp_app_podcast_library_episode_lookup, nmp_app_podcast_library_followed_podcasts,
    nmp_app_podcast_library_owned_podcasts, nmp_app_podcast_library_podcast_for_owner_pubkey,
    nmp_app_podcast_library_podcast_stats, nmp_app_podcast_library_show_episodes,
    nmp_app_podcast_library_starred_episodes, nmp_app_podcast_library_subscription_status,
    nmp_app_podcast_library_summary,
    nmp_app_podcast_local_model_catalog,
    nmp_app_podcast_local_search,
    nmp_app_podcast_openrouter_whisper_transcribe,
    nmp_app_podcast_perplexity_search, nmp_app_podcast_provider_model_catalog,
    nmp_app_podcast_register, nmp_app_podcast_rerank, nmp_app_podcast_set_data_dir,
    nmp_app_podcast_snapshot, nmp_app_podcast_snapshot_free, nmp_app_podcast_snapshot_rev,
    nmp_app_podcast_storage_breakdown, nmp_app_podcast_transcript_auto_ingest_candidates,
    nmp_app_podcast_transcript_ingest_plan,
    nmp_app_podcast_speech_model_catalog, nmp_app_podcast_threading_active_topics,
    nmp_app_podcast_threading_projection, nmp_app_podcast_unregister,
    nmp_app_podcast_validate_elevenlabs_key, nmp_app_podcast_validate_openrouter_key,
    nmp_app_podcast_voice_report, PodcastHandle,
};
pub use nmp_ffi::{
    nmp_app_cancel_bunker_handshake, nmp_app_nostrconnect_uri, nmp_free_string,
    nmp_signer_broker_init,
};
pub use player::{PlayerActor, PlayerState};
pub use queue::PlaybackQueue;

// Headless-scenario test surface: re-export the agent-note type so the
// headless binary can construct `CachedAgentNote` values for injection via
// `PodcastHandle::headless_inject_agent_note`.  Guarded by the `headless`
// feature flag so it does not widen the public API in production builds.
#[cfg(feature = "headless")]
pub use agent_note_handler::CachedAgentNote;

pub(crate) const PODCAST_FEEDBACK_PROJECT_COORDINATE: &str =
    "31933:09d48a1a5dbe13404a729634f1d6ba722d40513468dd713c8ea38ca9b7b6f2c7:podcast";
pub(crate) const PODCAST_FEEDBACK_INTEREST_NAMESPACE: &str = "podcast.feedback";
