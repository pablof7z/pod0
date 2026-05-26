//! Nostr relay capability wire types — `nostr_relay` namespace.
//!
//! Mirrors the `capability::http` layout: plain Serialize/Deserialize structs
//! for the request and result envelopes exchanged between the kernel and the
//! platform-side capability executor.
//!
//! ## Wire format
//!
//! The kernel serialises a `NostrRelayRequest` as the `payload_json` field of
//! a `CapabilityRequest`. The executor deserialises it, performs the network
//! operation (publish or subscribe), and returns a `NostrRelayResult` as the
//! `result_json` field of the corresponding `CapabilityEnvelope`.
//!
//! Both enums use `#[serde(tag = "type")]` so the discriminant is a top-level
//! `"type"` key — consistent with the other capability namespaces in this
//! crate.

use serde::{Deserialize, Serialize};

/// Capability namespace identifier for all Nostr relay operations.
pub const NOSTR_RELAY_CAPABILITY_NAMESPACE: &str = "nostr_relay";

/// Requests the kernel can issue to the Nostr relay executor.
#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type")]
pub enum NostrRelayRequest {
    /// Broadcast a pre-signed Nostr event to one or more relays.
    Publish {
        /// Fully-formed, signed Nostr event as a JSON string.
        event_json: String,
        /// Relay WebSocket URLs to publish to (e.g. `["wss://relay.primal.net"]`).
        relay_urls: Vec<String>,
    },
    /// Subscribe to a filter and collect events until EOSE or timeout.
    Subscribe {
        /// Nostr subscription ID (any unique string).
        sub_id: String,
        /// NIP-01 filter object (e.g. `{"kinds":[1],"authors":["..."]}`)
        filter: serde_json::Value,
        /// Relay WebSocket URLs to subscribe from.
        relay_urls: Vec<String>,
        /// How long to wait for EOSE before closing the subscription (ms).
        timeout_ms: u64,
    },
}

/// Results the Nostr relay executor returns to the kernel.
#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type")]
pub enum NostrRelayResult {
    /// Outcome of a `Publish` request.
    Published {
        /// Overall success (true if at least one relay accepted the event).
        ok: bool,
        /// Relay URLs that returned `OK true`.
        accepted_relays: Vec<String>,
        /// Per-relay errors: `(relay_url, error_message)` pairs for rejections.
        errors: Vec<(String, String)>,
    },
    /// Events collected from a `Subscribe` request.
    Events {
        /// Raw event JSON values received before EOSE (or timeout).
        events: Vec<serde_json::Value>,
        /// True if the relay sent an EOSE message before the timeout elapsed.
        eose: bool,
    },
    /// Top-level transport / parse error.
    Error { message: String },
}
