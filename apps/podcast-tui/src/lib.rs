//! `podcast-tui` — Ratatui terminal player for the Podcast app.
//!
//! Boots the NMP kernel via `nmp-app-podcast`, renders the JSON snapshot
//! as a terminal UI, and dispatches keyboard-driven actions back to the
//! kernel.  Audio playback is handled by an `mpv` subprocess capability
//! host (falls back to a stub on systems without mpv).

pub mod app;
pub mod audio_host;
pub mod bridge;
pub mod input;
pub mod runtime;
pub mod ui;
