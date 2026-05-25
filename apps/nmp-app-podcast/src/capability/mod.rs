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

pub mod audio;
pub mod dispatch;
pub mod download;

pub use audio::{AudioCommand, AudioReport, AUDIO_CAPABILITY_NAMESPACE};
pub use dispatch::{dispatch_audio_report_json, encode_audio_command, DispatchOutcome};
pub use download::{DownloadCommand, DownloadReport, DOWNLOAD_CAPABILITY_NAMESPACE};
