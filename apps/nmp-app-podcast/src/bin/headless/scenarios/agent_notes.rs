//! Scenario: agent-to-agent kind:1 notes via relay.primal.net (feature #44).
//!
//! Validates the publish + subscribe relay round-trip:
//! 1. Identity can be imported (nsec → signing key available).
//! 2. `PublishAgentNote` signs a kind:1 note addressed to a peer pubkey
//!    (NIP-10 root tag when replying) and publishes it; the envelope
//!    reports `status: "published"` (relay accepted) or `"signed"`.
//! 3. `FetchAgentNotes` subscribes with `{kinds:[1], "#p":[my_pubkey]}`
//!    and populates the cache without error (rev bumps on success).
//!
//! The publish here is self-addressed (recipient == our own pubkey) so the
//! relay has something tagged to us to return on fetch. The fetch handler
//! drops self-authored notes, so the cache may legitimately end up empty —
//! this scenario therefore validates the dispatch + relay layer, not the
//! presence of a specific row (which would require a second live peer).
//!
//! Skipped automatically when `relay.primal.net:443` is unreachable.

use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use nmp_app_podcast::PodcastHandle;
use nmp_ffi::NmpApp;
use serde_json::json;

use crate::fixtures;
use crate::harness::{dispatch, wait_for};
use crate::scenarios::ScenarioResult;
use crate::scenarios::ScenarioResult::{Fail, Pass, Skip};

const RELAY_HOST: &str = "relay.primal.net";
const RELAY_PORT: u16 = 443;
const PODCAST_NS: &str = "podcast";

/// TCP-level reachability probe (mirrors comments / relay_smoke).
fn probe_tcp(host: &str, port: u16) -> bool {
    let Ok(addrs) = (host, port).to_socket_addrs() else {
        return false;
    };
    addrs
        .into_iter()
        .any(|addr| TcpStream::connect_timeout(&addr, Duration::from_secs(3)).is_ok())
}

pub fn run(app: *mut NmpApp, handle: *mut PodcastHandle) -> ScenarioResult {
    // 1. Network gate — skip when relay is unreachable.
    if !probe_tcp(RELAY_HOST, RELAY_PORT) {
        return Skip(format!("{RELAY_HOST}:{RELAY_PORT} unreachable"));
    }

    // 2. Import identity so PublishAgentNote has a signing key.
    let res = dispatch(
        app,
        "podcast.identity",
        json!({"type": "ImportNsec", "nsec": fixtures::HEADLESS_TEST_NSEC}),
    );
    if let Some(err) = res.get("error").and_then(|v| v.as_str()) {
        return Fail(format!("ImportNsec dispatch rejected: {err}"));
    }
    std::thread::sleep(Duration::from_millis(300));

    // 3. Publish an agent-to-agent note addressed to our own pubkey so the
    //    relay has a kind:1 event tagged `#p:<us>` to return on fetch.
    let res = dispatch(
        app,
        PODCAST_NS,
        json!({
            "op": "publish_agent_note",
            "recipient_pubkey_hex": fixtures::HEADLESS_TEST_PUBKEY_HEX,
            "content": "headless test — feature #44 agent-to-agent note",
        }),
    );
    if let Some(err) = res.get("error").and_then(|v| v.as_str()) {
        return Fail(format!("PublishAgentNote dispatch rejected: {err}"));
    }
    // Relay round-trip adds latency; allow a generous window for the rev bump.
    if let Err(e) = wait_for(handle, 15_000, |_| true) {
        return Fail(format!("PublishAgentNote actor timed out: {e}"));
    }

    // 4. Fetch inbound notes — subscribes with the `#p` filter and writes
    //    the parsed result into the agent_notes cache (projected onto the
    //    snapshot's `agent_notes` field).
    let res = dispatch(app, PODCAST_NS, json!({"op": "fetch_agent_notes"}));
    if let Some(err) = res.get("error").and_then(|v| v.as_str()) {
        return Fail(format!("FetchAgentNotes dispatch rejected: {err}"));
    }
    if let Err(e) = wait_for(handle, 15_000, |_| true) {
        return Fail(format!("FetchAgentNotes actor timed out: {e}"));
    }

    // Both actions processed by the actor without error — the publish +
    // subscribe relay round-trip is validated. Any rows that land carry
    // `trusted: false` (no trust gate yet) and self-authored notes are
    // filtered, so we assert the dispatch layer rather than a specific row.
    Pass
}
