//! Scenario: fetch kind:3 contact list and kind:0 profile metadata.
//!
//! ## What it validates
//!
//! 1. Pre-publish a kind:3 event (via nak) that makes the test keypair follow
//!    two well-known Nostr accounts (fiatjaf + jb55) on relay.primal.net.
//! 2. Import the test identity so `active_account` is present.
//! 3. Dispatch `podcast.FetchContacts`.
//! 4. Wait up to 15 seconds for `PodcastUpdate.social.following_count >= 2`.
//! 5. Assert that `following` is non-empty (kind:0 metadata was hydrated).
//!
//! Skipped automatically when:
//! - `relay.primal.net:443` is TCP-unreachable (offline / CI).
//! - `nak` binary is not found at `/Users/pablofernandez/go/bin/nak`.

use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use nmp_app_podcast::PodcastHandle;
use nmp_ffi::NmpApp;
use serde_json::json;

use crate::fixtures;
use crate::harness::{dispatch, wait_for};
use crate::scenarios::ScenarioResult::{self, Fail, Pass, Skip};

const NAK_BIN: &str = "/Users/pablofernandez/go/bin/nak";
const RELAY_HOST: &str = "relay.primal.net";
const RELAY_PORT: u16 = 443;

/// fiatjaf — well-known Nostr account; verified via `nak decode npub180cvv07…`
const FIATJAF_HEX: &str = "3bf0c63fcb93463407af97a5e5ee64fa883d107ef9e558472c4eb9aaaefa459d";
/// jb55 — well-known Nostr account; verified via `nak decode npub1xtscya34g…`
const JB55_HEX: &str = "32e1827635450ebb3c5a7d12c1f8e7b2b514439ac10a67eef3d9fd9c5c68e245";

fn probe_tcp(host: &str, port: u16) -> bool {
    let Ok(addrs) = (host, port).to_socket_addrs() else {
        return false;
    };
    addrs.into_iter().any(|addr| {
        TcpStream::connect_timeout(&addr, Duration::from_secs(3)).is_ok()
    })
}

pub fn run(app: *mut NmpApp, handle: *mut PodcastHandle) -> ScenarioResult {
    // Network availability check.
    if !probe_tcp(RELAY_HOST, RELAY_PORT) {
        return Skip(format!("{RELAY_HOST}:{RELAY_PORT} unreachable"));
    }

    // nak availability check.
    if !std::path::Path::new(NAK_BIN).exists() {
        return Skip(format!("nak not found at {NAK_BIN}"));
    }

    // Pre-publish a kind:3 event so the test keypair follows fiatjaf + jb55.
    let nak_output = std::process::Command::new(NAK_BIN)
        .args([
            "event",
            "--sec", fixtures::HEADLESS_TEST_SECRET_HEX,
            "-k", "3",
            "-t", &format!("p={FIATJAF_HEX}"),
            "-t", &format!("p={JB55_HEX}"),
            &format!("wss://{RELAY_HOST}"),
        ])
        .output();

    match &nak_output {
        Ok(o) if !o.status.success() => {
            let stderr = String::from_utf8_lossy(&o.stderr);
            return Skip(format!("nak publish failed (relay may be offline): {stderr}"));
        }
        Err(e) => return Fail(format!("nak exec error: {e}")),
        Ok(_) => {}
    }

    // Ensure the test identity is imported. If a prior scenario already set
    // active_account (identity_import runs before us), take the fast path and
    // read the current snapshot directly rather than re-dispatching the same
    // nsec (the kernel deduplicates identical dispatches within its TTL window,
    // so a re-dispatch of the same action may be silently dropped, making
    // wait_for time out waiting for a rev change that never comes).
    use crate::harness::snapshot;
    let has_identity = snapshot(handle)
        .as_ref()
        .and_then(|u| u.active_account.as_ref())
        .is_some();

    if !has_identity {
        let res = dispatch(
            app,
            "podcast.identity",
            json!({"type": "ImportNsec", "nsec": fixtures::HEADLESS_TEST_NSEC}),
        );
        if let Some(err) = res.get("error").and_then(|v| v.as_str()) {
            return Fail(format!("ImportNsec rejected: {err}"));
        }
        match wait_for(handle, 5_000, |u| u.active_account.is_some()) {
            Err(e) => return Fail(format!("identity not set: {e}")),
            Ok(_) => {}
        }
    }

    // Dispatch FetchContacts.
    dispatch(app, "podcast", json!({"op": "fetch_contacts"}));

    // Wait for social snapshot with at least 2 follows.
    match wait_for(handle, 15_000, |u| {
        u.social
            .as_ref()
            .is_some_and(|s| s.following_count >= 2)
    }) {
        Ok(u) => {
            let social = u.social.unwrap();
            if social.following.is_empty() {
                return Fail("following_count >= 2 but following vec is empty".into());
            }
            Pass
        }
        Err(e) => Fail(e),
    }
}
