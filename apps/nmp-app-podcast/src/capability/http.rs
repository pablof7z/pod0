//! Podcast-app HTTP capability re-export — `nmp.http.capability`.
//!
//! This module exists to keep `capability::http` parallel to
//! `capability::audio` / `capability::download`. The actual types live in
//! [`podcast_feeds::http`] because `podcast-feeds` is the first low-level
//! consumer (RSS refresh, OPML probe in M2.B+C) and the kernel crate graph
//! is layered so `podcast-feeds` cannot back-dep on `nmp-app-podcast`. The
//! per-app crate re-exports them here so kernel modules that already
//! import from `crate::capability::{audio, download}` can pull HTTP from
//! the same path without learning a new crate name.
//!
//! ## Doctrine
//!
//! See [`podcast_feeds::http`] for the wire format, D6/D7 reasoning, and
//! the response-headers / case-insensitive lookup contract. Nothing new
//! happens at this layer.

pub use podcast_feeds::http::{
    HttpCommand, HttpMethod, HttpReport, HttpRequest, HttpResult,
    HTTP_ASYNC_CAPABILITY_NAMESPACE, HTTP_CAPABILITY_NAMESPACE,
};

#[cfg(test)]
#[path = "http_tests.rs"]
mod tests;
