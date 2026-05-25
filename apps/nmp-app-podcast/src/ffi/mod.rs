//! Podcast per-app FFI surface.
//!
//! `extern "C"` symbols Swift links against:
//!
//! - [`nmp_app_podcast_register`] — wire `nmp-app-template` defaults into
//!   the supplied `NmpApp` and return an opaque handle for subsequent
//!   snapshot / unregister calls.
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
//! * **No business logic in Swift** — Swift takes the JSON string, decodes
//!   to the appropriate types, and renders. All logic happens in Rust.
//!
//! ## Module layout
//!
//! Split across sub-modules to keep each file under the 500-LOC hard ceiling.
//! Every `pub extern "C"` symbol Swift links against is re-exported below.

pub mod actions;
mod handle;
mod helpers;
mod register;
mod snapshot;

pub use actions::{
    CancelAllDownloadsAction, CancelDownloadAction, DownloadEpisodeAction, PauseAction,
    PauseDownloadAction, PlayAction, ResumeDownloadAction, SeekAction, SetSleepTimerAction,
    SetSpeedAction, SetVolumeAction, StopAction, ACTION_PLAYER_CANCEL_ALL_DOWNLOADS,
    ACTION_PLAYER_CANCEL_DOWNLOAD, ACTION_PLAYER_DOWNLOAD, ACTION_PLAYER_PAUSE,
    ACTION_PLAYER_PAUSE_DOWNLOAD, ACTION_PLAYER_PLAY, ACTION_PLAYER_RESUME_DOWNLOAD,
    ACTION_PLAYER_SEEK, ACTION_PLAYER_SET_SLEEP_TIMER, ACTION_PLAYER_SET_SPEED,
    ACTION_PLAYER_SET_VOLUME, ACTION_PLAYER_STOP,
};
pub use handle::PodcastHandle;
pub use register::nmp_app_podcast_register;
pub use snapshot::{
    nmp_app_podcast_snapshot, nmp_app_podcast_snapshot_free, nmp_app_podcast_unregister,
    DownloadItemSnapshot, DownloadQueueSnapshot, PodcastUpdate,
};
