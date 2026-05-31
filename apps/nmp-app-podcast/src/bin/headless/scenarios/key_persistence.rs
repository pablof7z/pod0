//! Scenario: create an owned podcast and verify a real NIP-F4 keypair is returned.
//!
//! This scenario proves that `podcast.publish.create_owned_podcast` populates
//! `owned_podcasts` in the snapshot with a valid 64-character hex pubkey for a
//! subscribed podcast.
//!
//! Note: `nmp_app_dispatch_action` returns `{"correlation_id":"..."}` (accepted
//! + enqueued), never the handler result. The actual result appears in the
//! snapshot via `owned_podcasts`. We poll the snapshot until that entry appears.
//!
//! The full round-trip durability proof (restart + reload of
//! `podcast-keys.json`) is covered by the unit test `keys_persist_and_reload`
//! in `store/podcast_keys_tests.rs` — doing it here would require two separate
//! NmpApp instances sharing a tempdir, which the current headless harness does
//! not support.

use nmp_app_podcast::PodcastHandle;
use nmp_ffi::NmpApp;

use crate::harness::{dispatch, wait_for};
use crate::mock_feed;
use crate::scenarios::ScenarioResult;

pub fn run(app: *mut NmpApp, handle: *mut PodcastHandle) -> ScenarioResult {
    // 1. Subscribe to a local mock RSS feed so we have a real podcast_id.
    let port = mock_feed::start();
    let feed_url = format!("http://127.0.0.1:{port}/feed.xml");

    let sub_result = dispatch(
        app,
        "podcast",
        serde_json::json!({"op": "subscribe", "feed_url": feed_url}),
    );
    if let Some(err) = sub_result.get("error").and_then(|v| v.as_str()) {
        return ScenarioResult::Fail(format!("subscribe rejected: {err}"));
    }

    // 2. Wait for the library to populate so we can read the podcast_id.
    let update = match wait_for(handle, 10_000, |u| !u.library.is_empty()) {
        Ok(u) => u,
        Err(msg) => return ScenarioResult::Fail(format!("timeout waiting for library: {msg}")),
    };

    let podcast_id = update.library[0].id.clone();

    // 3. Dispatch create_owned_podcast. The dispatch returns
    //    {"correlation_id": "..."} — the result lands asynchronously in the
    //    snapshot's `owned_podcasts` field once the actor processes the command.
    let dispatch_res = dispatch(
        app,
        "podcast.publish",
        serde_json::json!({"op": "create_owned_podcast", "podcast_id": podcast_id}),
    );
    if dispatch_res.get("error").is_some() {
        return ScenarioResult::Fail(format!("dispatch rejected: {dispatch_res}"));
    }

    // 4. Poll the snapshot until owned_podcasts contains our podcast_id.
    let target_id = podcast_id.clone();
    let update = match wait_for(handle, 10_000, |u| {
        u.owned_podcasts.iter().any(|o| o.podcast_id == target_id)
    }) {
        Ok(u) => u,
        Err(msg) => return ScenarioResult::Fail(format!(
            "timeout waiting for owned_podcasts to contain podcast_id={podcast_id}: {msg}"
        )),
    };

    // 5. Extract and validate the pubkey_hex.
    let owned = match update.owned_podcasts.iter().find(|o| o.podcast_id == podcast_id) {
        Some(o) => o,
        None => return ScenarioResult::Fail("owned_podcasts entry disappeared".into()),
    };

    let pubkey = &owned.podcast_pubkey_hex;
    if pubkey.len() != 64 {
        return ScenarioResult::Fail(format!(
            "expected 64-char pubkey_hex, got {} chars: {pubkey}",
            pubkey.len()
        ));
    }

    if !pubkey.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()) {
        return ScenarioResult::Fail(format!("pubkey_hex is not lowercase hex: {pubkey}"));
    }

    ScenarioResult::Pass
}
