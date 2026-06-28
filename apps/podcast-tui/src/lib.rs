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
mod local_model_catalog;
mod navigation;
mod provider_model_catalog;
mod provider_setting_model_selection;
mod provider_settings_catalog;
mod provider_settings_parser;
mod provider_voice_catalog;
pub mod rows;
pub mod runtime;
mod runtime_actions;
mod runtime_actions_nostr;
mod runtime_settings_actions;
pub mod settings_catalog;
mod settings_state;
mod speech_model_catalog;
pub mod ui;
mod update;
