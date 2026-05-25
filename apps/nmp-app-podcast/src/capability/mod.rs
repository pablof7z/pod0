//! Podcast-app capability contracts.
//!
//! These modules define the request/event vocabularies for capabilities the
//! podcast app drives but the canonical `nmp-core::capability` doesn't yet
//! ship. As each capability lands upstream in `nostrmultiplatform`, the
//! podcast-local skeleton here will be reconciled against the canonical
//! contract (see the per-module doc comments for migration notes).
//!
//! Capabilities defined here:
//!
//! - [`audio`] — `nmp.audio.capability` (M3.A; iOS executor in M3.C).
//! - [`download`] — `nmp.download.capability` (M4.A; iOS executor in M4.C).
//! - [`http`] — `nmp.http.capability` (M5; iOS executor already lives at
//!   `ios/Podcast/Podcast/Capabilities/HttpCapability.swift` since M0.C and
//!   was implemented before the Rust contract existed). The types here are
//!   re-exported from `podcast-feeds::http`; see that module for the wire
//!   format.
//! - [`voice`] — `nmp.voice.capability` (M8.A; iOS executor in M8.C).

pub mod audio;
pub mod dispatch;
pub mod download;
pub mod http;
pub mod voice;

pub use audio::{AudioCommand, AudioReport, AUDIO_CAPABILITY_NAMESPACE};
pub use dispatch::{dispatch_audio_report_json, encode_audio_command, DispatchOutcome};
pub use download::{DownloadCommand, DownloadReport, DOWNLOAD_CAPABILITY_NAMESPACE};
pub use http::{HttpMethod, HttpRequest, HttpResult, HTTP_CAPABILITY_NAMESPACE};
pub use voice::{VoiceCommand, VoiceReport, VOICE_CAPABILITY_NAMESPACE};
