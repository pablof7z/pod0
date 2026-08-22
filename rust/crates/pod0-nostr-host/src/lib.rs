#![forbid(unsafe_code)]

//! Blocking, pure-Rust execution of one exact leased Pod0 Nostr publication.
//!
//! Success requires distinct configured relays to return NIP-01
//! `["OK", event_id, true, ...]` frames for the signed event. A socket write,
//! disconnect, timeout, or `OK false` never becomes success.
//!
//! This crate is a CLI/platform executor. It does not compose product events,
//! select relays, retry, persist custody, or mint NMP receipt IDs. In
//! particular, [`AcknowledgedPublication`] is relay evidence and must not be
//! converted into `LeasedNMPPublicationReceipt`; only NMP can issue that ID.
//! Tungstenite formats complete frames at trace level, so this crate statically
//! caps the shared `log` facade at `Info` to make payload logging impossible.

mod cancellation;
mod config;
mod error;
mod event;
mod evidence;
mod publisher;
mod relay;
mod secure_hash;
mod signing;

pub use cancellation::CancellationToken;
pub use config::{AcknowledgementThreshold, PublisherConfig};
pub use error::{ConfigurationError, DraftError, PublishError, SigningSecretError};
pub use evidence::{
    AcknowledgedPublication, AmbiguityCause, PublicationFailure, RelayFailureCode,
    RelayFailureStage, RelayOutcome,
};
pub use publisher::NostrPublisher;
pub use signing::SigningSecret;

pub use pod0_application::LeasedNMPPublicationDraft;
