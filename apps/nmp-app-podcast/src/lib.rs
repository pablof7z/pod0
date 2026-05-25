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
//! The shell calls [`nmp_signer_broker_init`] once after `nmp_app_new`, then
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

pub(crate) mod briefings_handler;
pub(crate) mod ai_chapters;
pub mod capability;
pub(crate) mod chapter;
pub(crate) mod discover_nostr;
pub(crate) mod comments_handler;
pub mod download;
pub mod ffi;
pub(crate) mod host_op_handler;
pub(crate) mod host_op_handler_helpers;
pub(crate) mod inbox_handler;
pub(crate) mod itunes_search;
pub(crate) mod host_op_helpers;
pub(crate) mod host_op_helpers;
pub(crate) mod picks_handler;
pub mod knowledge;
pub(crate) mod itunes_search;
pub(crate) mod memory_handler;
pub(crate) mod host_op_helpers;
pub(crate) mod clip_handler;
pub mod download;
pub mod ffi;
pub(crate) mod host_op_handler;
pub(crate) mod itunes;
pub(crate) mod host_op_itunes;
pub(crate) mod host_op_publish;
pub(crate) mod itunes_helpers;
pub mod player;
pub(crate) mod social_handler;
pub(crate) mod host_op_handler_itunes;
pub(crate) mod host_op_handler_queue;
pub mod queue;
pub(crate) mod player_handler;
pub mod store;
pub(crate) mod transcript;
pub(crate) mod wiki;
pub(crate) mod tasks_handler;
pub(crate) mod tts;
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
    nmp_app_podcast_audio_report, nmp_app_podcast_register, nmp_app_podcast_set_data_dir,
    nmp_app_podcast_download_report,
    nmp_app_podcast_snapshot, nmp_app_podcast_snapshot_free, nmp_app_podcast_unregister,
    nmp_app_podcast_voice_report, PodcastHandle,
};
pub use nmp_signer_broker::{
    nmp_app_cancel_bunker_handshake, nmp_app_nostrconnect_uri, nmp_broker_free_string,
    nmp_signer_broker_init,
};
pub use player::{PlayerActor, PlayerState};
pub use queue::PlaybackQueue;
