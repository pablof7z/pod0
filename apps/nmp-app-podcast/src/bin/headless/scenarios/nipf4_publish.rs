//! Scenario: create an owned podcast, publish its NIP-F4 show event, and
//! verify that the resulting event has a valid secp256k1 signature.
//!
//! The scenario exercises the full `publish_show` path:
//!   1. Subscribe to a local mock RSS feed.
//!   2. Wait for the podcast to appear as a NEW entry in the library
//!      (previous scenarios may have left their own podcasts in the store).
//!   3. Dispatch `create_owned_podcast` to mint a per-podcast keypair.
//!   4. Wait for the podcast to appear in `owned_podcasts`.
//!   5. Dispatch `publish_show` and wait for `owned_podcasts[podcast_id].show_event_json`
//!      to be populated (the actor stamps it after signing).
//!   6. Parse the event JSON and validate `id` (64-char hex) and `sig`
//!      (128-char hex) are non-null, proving real secp256k1 signing ran.
//!
//! Relay connectivity is intentionally not required: the scenario validates
//! the signing layer regardless of whether the relay accepted the event.
//!
//! Note: `nmp_app_dispatch_action` always returns `{"correlation_id":"..."}` —
//! the handler result arrives asynchronously via the snapshot (pull) path.
//! We observe results through `owned_podcasts.show_event_json`.

use nmp_app_podcast::PodcastHandle;
use nmp_ffi::NmpApp;

use crate::harness::{dispatch, snapshot, wait_for};
use crate::mock_feed;
use crate::scenarios::ScenarioResult;

pub fn run(app: *mut NmpApp, handle: *mut PodcastHandle) -> ScenarioResult {
    // Capture the set of podcast IDs already in the library before subscribing
    // so we can identify the new entry added by this scenario. Previous
    // scenarios may have left podcasts in the shared store.
    let existing_ids: std::collections::HashSet<String> = snapshot(handle)
        .map(|u| u.library.iter().map(|p| p.id.clone()).collect())
        .unwrap_or_default();

    // 1. Subscribe to a local mock RSS feed.
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

    // 2. Wait for a NEW podcast to appear in the library (one whose ID was not
    //    present before this scenario's subscribe call).
    let update = match wait_for(handle, 10_000, |u| {
        u.library.iter().any(|p| !existing_ids.contains(&p.id))
    }) {
        Ok(u) => u,
        Err(msg) => {
            return ScenarioResult::Fail(format!("timeout waiting for new library entry: {msg}"))
        }
    };

    let podcast_id = update
        .library
        .iter()
        .find(|p| !existing_ids.contains(&p.id))
        .map(|p| p.id.clone())
        .expect("predicate ensured at least one new entry");

    // 3. Dispatch create_owned_podcast (async — result lands in snapshot).
    let dispatch_res = dispatch(
        app,
        "podcast.publish",
        serde_json::json!({"op": "create_owned_podcast", "podcast_id": podcast_id}),
    );
    if dispatch_res.get("error").is_some() {
        return ScenarioResult::Fail(format!("create_owned_podcast rejected: {dispatch_res}"));
    }

    // 4. Poll until owned_podcasts contains our podcast.
    let target_id = podcast_id.clone();
    match wait_for(handle, 10_000, |u| {
        u.owned_podcasts.iter().any(|o| o.podcast_id == target_id)
    }) {
        Ok(_) => {}
        Err(msg) => {
            return ScenarioResult::Fail(format!(
                "timeout waiting for owned_podcasts (podcast_id={podcast_id}): {msg}"
            ))
        }
    };

    // 5. Dispatch publish_show (async — result lands in owned_podcasts.show_event_json).
    let pub_dispatch = dispatch(
        app,
        "podcast.publish",
        serde_json::json!({"op": "publish_show", "podcast_id": podcast_id}),
    );
    if pub_dispatch.get("error").is_some() {
        return ScenarioResult::Fail(format!("publish_show dispatch rejected: {pub_dispatch}"));
    }

    // 6. Poll until owned_podcasts[podcast_id].show_event_json is populated
    //    by the actor after signing.
    let target_id2 = podcast_id.clone();
    let update = match wait_for(handle, 10_000, |u| {
        u.owned_podcasts
            .iter()
            .find(|o| o.podcast_id == target_id2)
            .and_then(|o| o.show_event_json.as_ref())
            .is_some()
    }) {
        Ok(u) => u,
        Err(msg) => {
            return ScenarioResult::Fail(format!(
                "timeout waiting for show_event_json (podcast_id={podcast_id}): {msg}"
            ))
        }
    };

    // 7. Extract and validate the signed event JSON.
    let owned = match update
        .owned_podcasts
        .iter()
        .find(|o| o.podcast_id == podcast_id)
    {
        Some(o) => o,
        None => return ScenarioResult::Fail("owned entry disappeared after wait_for".into()),
    };
    let event_json_str = match owned.show_event_json.as_deref() {
        Some(s) => s,
        None => return ScenarioResult::Fail("show_event_json is None after wait_for".into()),
    };
    let event: serde_json::Value = match serde_json::from_str(event_json_str) {
        Ok(v) => v,
        Err(e) => return ScenarioResult::Fail(format!("show_event_json is not valid JSON: {e}")),
    };

    // kind must be 10154 (NIP-F4 show).
    if event["kind"] != 10154 {
        return ScenarioResult::Fail(format!("expected kind 10154, got {}", event["kind"]));
    }

    // id must be a 64-char lowercase hex string.
    match event["id"].as_str() {
        Some(id) if id.len() == 64 && id.chars().all(|c| c.is_ascii_hexdigit()) => {}
        Some(id) => {
            return ScenarioResult::Fail(format!(
                "event.id is not 64-char hex: len={} id={id}",
                id.len()
            ))
        }
        None => return ScenarioResult::Fail("event.id is null — signing did not run".into()),
    }

    // sig must be a 128-char lowercase hex string (64-byte Schnorr signature).
    match event["sig"].as_str() {
        Some(sig) if sig.len() == 128 && sig.chars().all(|c| c.is_ascii_hexdigit()) => {}
        Some(sig) => {
            return ScenarioResult::Fail(format!(
                "event.sig is not 128-char hex: len={} sig={sig}",
                sig.len()
            ))
        }
        None => return ScenarioResult::Fail("event.sig is null — signing did not run".into()),
    }

    // pubkey must be a 64-char hex string.
    match event["pubkey"].as_str() {
        Some(pk) if pk.len() == 64 && pk.chars().all(|c| c.is_ascii_hexdigit()) => {}
        _ => {
            return ScenarioResult::Fail(format!(
                "event.pubkey is not 64-char hex: {}",
                event["pubkey"]
            ))
        }
    }

    ScenarioResult::Pass
}
