//! `podcast-tui` — Ratatui terminal player for the Podcast app.
//!
//! Boots the NMP kernel via `nmp-app-podcast`, renders the JSON snapshot
//! as a terminal UI, and dispatches keyboard-driven actions back to the
//! kernel.  Audio playback is handled by an `mpv` subprocess capability
//! host (falls back to a stub on systems without mpv).

mod agent_state;
pub mod app;
pub mod audio_host;
pub mod bridge;
mod download_state;
pub mod input;
mod navigation;
mod provider_settings_catalog;
mod provider_settings_parser;
pub mod rows;
pub mod runtime;
mod runtime_actions;
mod runtime_settings_actions;
pub mod settings_catalog;
mod settings_state;
pub mod ui;
mod update;
