//! Scenario: dispatch `podcast.discover_nostr` and verify kind:10154 results
//! arrive from `relay.primal.net`.
//!
//! Skipped automatically when `relay.primal.net:443` is unreachable (CI or
//! no-network environments). Uses the real WebSocket relay capability path
//! that PR 9 wired into the `DiscoverNostr` handler.
//!
//! ## Race-safe polling
//!
//! `wait_for` detects rev changes after `last_rev` is sampled. When the
//! actor completes the action (relay 8 s timeout + HTTP fallback) between
//! `dispatch()` returning and `wait_for` reading `last_rev`, the rev bump
//! is invisible to the poller. We therefore also read the snapshot directly
//! after any `wait_for` timeout to catch the "already done" case.

use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use nmp_app_podcast::PodcastHandle;
use nmp_ffi::NmpApp;

use crate::harness::{dispatch, snapshot, wait_for};
use crate::scenarios::ScenarioResult;
use crate::scenarios::ScenarioResult::{Fail, Pass, Skip};

const RELAY_HOST: &str = "relay.primal.net";
const RELAY_PORT: u16 = 443;

/// TCP-level reachability probe — resolves the hostname and attempts a
/// connection within 3 s. Returns `true` if any resolved address is reachable.
fn probe_tcp(host: &str, port: u16) -> bool {
    let Ok(addrs) = (host, port).to_socket_addrs() else {
        return false;
    };
    addrs
        .into_iter()
        .any(|addr| TcpStream::connect_timeout(&addr, Duration::from_secs(3)).is_ok())
}

pub fn run(app: *mut NmpApp, handle: *mut PodcastHandle) -> ScenarioResult {
    if !probe_tcp(RELAY_HOST, RELAY_PORT) {
        return Skip(format!("{RELAY_HOST}:{RELAY_PORT} unreachable"));
    }

    // Dispatch a no-query discovery sweep — kind:10154 browse on relay.primal.net.
    let result = dispatch(
        app,
        "podcast",
        serde_json::json!({
            "op": "discover_nostr",
            "consumer_id": "headless-discover-nostr",
            "query": null
        }),
    );

    if let Some(err) = result.get("error").and_then(|v| v.as_str()) {
        return Fail(format!("dispatch rejected: {err}"));
    }

    // Wait for nostr_results to be non-empty (40 s ceiling: relay uses 8 s
    // timeout, HTTP fallback adds ~5 s, plus actor scheduling jitter).
    //
    // If wait_for times out, do a direct snapshot read: the action may have
    // completed before wait_for sampled last_rev (actor can be that fast when
    // relay returns EOSE quickly and HTTP fallback has low latency).
    let update = match wait_for(handle, 40_000, |u| !u.nostr_results.is_empty()) {
        Ok(u) => u,
        Err(_) => {
            // Race-safe fallback: read the current snapshot directly.
            match snapshot(handle) {
                Some(snap) if !snap.nostr_results.is_empty() => snap,
                _ => {
                    return Fail(
                        "nostr_results still empty after 40 s; \
                         relay returned no kind:10154 events and HTTP fallback also empty"
                            .into(),
                    );
                }
            }
        }
    };

    let first = &update.nostr_results[0];
    if first.title.is_empty() {
        return Fail(format!(
            "first result has empty title (event_id={})",
            first.event_id
        ));
    }

    Pass
}
