//! Scenario: publish a test event via nak and verify the Nostr relay accepts it.
//!
//! Uses the `nak` CLI to sign and publish a kind-1 event to `relay.primal.net`,
//! then confirms the event ID is non-empty. This validates end-to-end relay
//! connectivity and event publishing without requiring the full relay capability
//! to be invoked through the kernel (that path is tested in PR 9 discover).
//!
//! The scenario is skipped automatically when:
//! - `relay.primal.net:443` is unreachable (no network / CI without relay)
//! - the `nak` binary is not found at the expected path (env without nak)

use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use nmp_app_podcast::PodcastHandle;
use nmp_ffi::NmpApp;

use crate::fixtures::{HEADLESS_TEST_PUBKEY_HEX, HEADLESS_TEST_SECRET_HEX};
use crate::scenarios::ScenarioResult;
use crate::scenarios::ScenarioResult::{Fail, Pass, Skip};

const NAK_BIN: &str = "/Users/pablofernandez/go/bin/nak";
const RELAY_HOST: &str = "relay.primal.net";
const RELAY_PORT: u16 = 443;

/// TCP-level reachability probe. Resolves the hostname via DNS, then attempts
/// a TCP connection to each resolved address. Returns `true` if any address
/// can be connected to within 3 seconds.
///
/// `SocketAddr::from_str` only accepts numeric IPs, so we must use
/// `ToSocketAddrs` to perform DNS resolution before connecting.
fn probe_tcp(host: &str, port: u16) -> bool {
    let Ok(addrs) = (host, port).to_socket_addrs() else {
        return false;
    };
    addrs.into_iter().any(|addr| {
        TcpStream::connect_timeout(&addr, Duration::from_secs(3)).is_ok()
    })
}

#[allow(unused_variables)]
pub fn run(app: *mut NmpApp, handle: *mut PodcastHandle) -> ScenarioResult {
    // 1. Network availability check.
    if !probe_tcp(RELAY_HOST, RELAY_PORT) {
        return Skip(format!("{RELAY_HOST}:{RELAY_PORT} unreachable"));
    }

    // 2. nak availability check — missing binary is a SKIP, not a FAIL,
    //    because environments without nak (e.g. CI) should not go red.
    if !std::path::Path::new(NAK_BIN).exists() {
        return Skip(format!("nak not found at {NAK_BIN}"));
    }

    // 3. Publish a kind-1 test event via nak and capture the event JSON.
    let nak_output = std::process::Command::new(NAK_BIN)
        .args([
            "event",
            "--sec", HEADLESS_TEST_SECRET_HEX,
            "-k", "1",
            "-c", "headless relay smoke test",
            &format!("wss://{RELAY_HOST}"),
        ])
        .output();

    let output = match nak_output {
        Ok(o) => o,
        Err(e) => return Fail(format!("nak exec error: {e}")),
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Fail(format!("nak exited non-zero: {stderr}"));
    }

    // 4. Extract and validate the event ID from nak's JSON output.
    let stdout = String::from_utf8_lossy(&output.stdout);
    let event_json: serde_json::Value = match serde_json::from_str(stdout.trim()) {
        Ok(v) => v,
        Err(e) => return Fail(format!("nak output not valid JSON: {e} — raw: {stdout}")),
    };

    let event_id = event_json["id"].as_str().unwrap_or("").to_string();
    if event_id.is_empty() {
        return Fail(format!("nak returned empty event id; output: {stdout}"));
    }

    // 5. Verify the pubkey in the event matches our test fixture.
    let event_pubkey = event_json["pubkey"].as_str().unwrap_or("");
    if event_pubkey != HEADLESS_TEST_PUBKEY_HEX {
        return Fail(format!(
            "event pubkey mismatch: expected {HEADLESS_TEST_PUBKEY_HEX}, got {event_pubkey}"
        ));
    }

    // Full relay capability round-trip (subscribe to fetch the published event)
    // is exercised in PR 9 (discover). For now, a successful publish is sufficient
    // to validate relay connectivity and the nak signing path.
    Pass
}
