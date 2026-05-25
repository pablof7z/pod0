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

pub mod capability;
pub mod ffi;
pub mod player;

pub use capability::{AudioCommand, AudioReport, AUDIO_CAPABILITY_NAMESPACE};
pub use ffi::{
    nmp_app_podcast_register, nmp_app_podcast_snapshot, nmp_app_podcast_snapshot_free,
    nmp_app_podcast_unregister, PodcastHandle,
};
pub use nmp_signer_broker::{
    nmp_app_cancel_bunker_handshake, nmp_app_nostrconnect_uri, nmp_broker_free_string,
    nmp_signer_broker_init,
};
pub use player::{PlayerActor, PlayerState};
