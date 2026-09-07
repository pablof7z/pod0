//! Real, self-contained media decoding, macOS playback, and WAV clip export.
//!
//! [`PlaybackObservation`] is intentionally host-neutral. A Pod0 host can map
//! its state, position, duration, and `ended` fields directly into
//! `PlaybackLifecycleObservation`, while supplying episode, route, and
//! interruption data owned by the host.

mod cancel;
mod clip;
mod decode;
mod error;
mod playback_backend;
mod player;
mod publish;
mod source;

pub use cancel::CancellationToken;
pub use clip::{ClipEncoding, ClipExport, ClipExportRequest, export_clip};
pub use decode::{MediaMetadata, PreparedMedia};
pub use error::{MediaError, Result};
pub use player::{
    MAX_PLAYBACK_RATE, MIN_PLAYBACK_RATE, MediaPlayer, PlaybackObservation, PlaybackState,
};
pub use source::{HttpLoadOptions, MediaLoader, MediaSource};
