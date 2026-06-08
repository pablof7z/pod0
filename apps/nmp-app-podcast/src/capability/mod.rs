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
//! - [`notification`] — `nmp.notification.capability` (feature #20; iOS
//!   executor in `ios/Podcast/Podcast/Capabilities/NotificationCapability.swift`).
//! - [`nostr_relay`] — `nostr_relay` capability (PR 7; headless executor in
//!   `bin/headless/relay_client.rs`; iOS executor arrives in PR 8+).
//! - [`voice`] — `nmp.voice.capability` (M8.A; iOS executor in M8.C).

pub mod audio;
pub mod dispatch;
pub mod download;
pub mod http;
pub mod network;
pub mod nostr_relay;
pub mod notification;
pub mod voice;

pub use audio::{AudioCommand, AudioReport, AUDIO_CAPABILITY_NAMESPACE};
pub use dispatch::{
    dispatch_audio_report_json, dispatch_download_report_json, encode_audio_command,
    DispatchOutcome,
};
pub use download::{DownloadCommand, DownloadKind, DownloadReport, DOWNLOAD_CAPABILITY_NAMESPACE};
pub use http::{
    HttpCommand, HttpMethod, HttpReport, HttpRequest, HttpResult,
    HTTP_ASYNC_CAPABILITY_NAMESPACE, HTTP_CAPABILITY_NAMESPACE,
};
pub use network::{NetworkReport, NETWORK_CAPABILITY_NAMESPACE};
pub use nostr_relay::{NostrRelayRequest, NostrRelayResult, NOSTR_RELAY_CAPABILITY_NAMESPACE};
pub use notification::{
    notification_command_json, NotificationCommand, NOTIFICATION_CAPABILITY_NAMESPACE,
};
pub use voice::{VoiceCommand, VoiceReport, VOICE_CAPABILITY_NAMESPACE};
