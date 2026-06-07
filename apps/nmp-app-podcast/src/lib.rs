//! `nmp-app-podcast` — Podcast per-app glue.
//!
//! Composes `nmp-core` (the kernel substrate + event observer slot) with
//! `nmp-app-template` (the canonical NMP composition root) to surface podcast
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
//! 1. Wires the canonical NMP defaults via `nmp_app_template::register_defaults`.
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
pub(crate) mod agent_note_handler;
pub(crate) mod agent_llm;
pub(crate) mod agent_tools;
pub(crate) mod identity_handler;
pub(crate) mod ai_chapters;
pub(crate) mod ai_chapters_llm;
pub(crate) mod blossom;
pub mod capability;
pub(crate) mod categorization;
pub(crate) mod categorization_llm;
pub(crate) mod chapter;
pub(crate) mod clip_handler;
pub(crate) mod comments_anchor;
pub(crate) mod comments_handler;
pub(crate) mod discover_nostr;
pub(crate) mod nmp_dispatch;
pub mod download;
pub(crate) mod episode_summary;
pub(crate) mod episode_summary_llm;
pub(crate) mod feedback_handler;
pub mod ffi;
pub(crate) mod host_op_handler;
pub(crate) mod host_op_handler_helpers;
pub(crate) mod host_op_handler_queue;
pub(crate) mod host_op_publish;
pub(crate) mod host_op_publish_lifecycle;
pub(crate) mod inbox_handler;
pub(crate) mod inbox_llm;
pub(crate) mod itunes;
pub mod knowledge;
pub mod llm;
pub(crate) mod memory_handler;
pub(crate) mod picks_handler;
pub(crate) mod picks_llm;
pub mod player;
pub mod queue;
pub(crate) mod relay;
pub(crate) mod snapshot_signal;
pub(crate) mod social_handler;
pub(crate) mod social_publish_handler;
pub mod store;
pub(crate) mod tasks_handler;
pub(crate) mod transcript;
pub(crate) mod voice_conversation;
pub(crate) mod voice_handler;
pub(crate) mod wiki;
pub(crate) mod wiki_llm;

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
    nmp_app_podcast_audio_report, nmp_app_podcast_download_report,
    nmp_app_podcast_episode_events, nmp_app_podcast_generate_image,
    nmp_app_podcast_openrouter_whisper_transcribe,
    nmp_app_podcast_provider_model_catalog, nmp_app_podcast_register, nmp_app_podcast_rerank,
    nmp_app_podcast_set_data_dir, nmp_app_podcast_snapshot, nmp_app_podcast_snapshot_free,
    nmp_app_podcast_snapshot_rev,
    nmp_app_podcast_unregister, nmp_app_podcast_voice_report, PodcastHandle,
    nmp_app_podcast_validate_elevenlabs_key, nmp_app_podcast_validate_openrouter_key,
};
pub use nmp_ffi::{
    nmp_app_cancel_bunker_handshake, nmp_app_nostrconnect_uri, nmp_broker_free_string,
    nmp_signer_broker_init,
};
pub use player::{PlayerActor, PlayerState};
pub use queue::PlaybackQueue;
