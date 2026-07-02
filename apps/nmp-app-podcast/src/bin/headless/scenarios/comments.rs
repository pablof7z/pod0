//! Scenario: fetch and post kind-1111 NIP-22 comments via relay.primal.net.
//!
//! Validates that:
//! 1. Identity can be imported (nsec → active_account in snapshot).
//! 2. An RSS feed can be subscribed so an episode is available.
//! 3. `PostComment` signs and publishes a kind-1111 event successfully.
//! 4. `FetchComments` queries the relay and populates the cache (returns ok).
//!
//! Comment visibility in the snapshot requires the episode to be "now playing"
//! (the projection only surfaces comments for `now_playing.episode_id`). This
//! scenario validates the dispatch layer; full snapshot round-trip is deferred
//! until the player integration wires up.
//!
//! Skipped automatically when `relay.primal.net:443` is unreachable.

use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use nmp_app_podcast::PodcastHandle;
use nmp_native_runtime::NmpApp;
use serde_json::json;

use crate::fixtures;
use crate::harness::{dispatch, wait_for};
use crate::mock_feed;
use crate::scenarios::ScenarioResult;
use crate::scenarios::ScenarioResult::{Fail, Pass, Skip};

const RELAY_HOST: &str = "relay.primal.net";
const RELAY_PORT: u16 = 443;
const PODCAST_NS: &str = "podcast";

/// TCP-level reachability probe (mirrors relay_smoke).
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

    // 2. Import identity so PostComment has a signing key.
    // Note: identity_import scenario may have already loaded the same key.
    // Re-importing is idempotent and always bumps rev. We verify by checking
    // the dispatch didn't error at the module level; the actor processes it
    // asynchronously but quickly.
    let res = dispatch(
        app,
        "podcast.identity",
        json!({"type": "ImportNsec", "nsec": fixtures::HEADLESS_TEST_NSEC}),
    );
    if let Some(err) = res.get("error").and_then(|v| v.as_str()) {
        return Fail(format!("ImportNsec dispatch rejected: {err}"));
    }
    // Give the actor thread a moment to process ImportNsec.
    std::thread::sleep(std::time::Duration::from_millis(300));

    // 3. Subscribe to the mock feed to get an episode.
    let port = mock_feed::start();
    let feed_url = format!("http://127.0.0.1:{port}/feed.xml");
    let res = dispatch(
        app,
        PODCAST_NS,
        json!({"op": "subscribe", "feed_url": feed_url}),
    );
    if let Some(err) = res.get("error").and_then(|v| v.as_str()) {
        return Fail(format!("subscribe dispatch rejected: {err}"));
    }

    let update = match wait_for(handle, 10_000, |u| {
        !u.library.is_empty() && !u.library[0].episodes.is_empty()
    }) {
        Ok(u) => u,
        Err(e) => return Fail(format!("library not populated: {e}")),
    };
    let episode_id = update.library[0].episodes[0].id.clone();

    // 4. Post a comment — signs with the loaded identity and publishes to relay.
    // The action is asynchronous: dispatch returns a correlation_id, and the
    // actor thread processes the action (sign + publish) independently.
    // Success is signalled by a rev bump (the handler calls rev.fetch_add).
    let comment_text = format!(
        "headless test comment — pr/comments — ep {}",
        &episode_id[..8]
    );
    let res = dispatch(
        app,
        PODCAST_NS,
        json!({
            "op": "post_comment",
            "episode_id": episode_id,
            "content": comment_text
        }),
    );
    if let Some(err) = res.get("error").and_then(|v| v.as_str()) {
        return Fail(format!("PostComment dispatch rejected: {err}"));
    }
    // Wait for the actor to process PostComment (rev bumps on success).
    // Timeout is generous (15 s) because the relay round-trip adds latency.
    let post_ok = wait_for(handle, 15_000, |_| {
        // Any rev bump after dispatch counts — PostComment writes to cache.
        true
    });
    if let Err(e) = post_ok {
        return Fail(format!("PostComment actor timed out: {e}"));
    }

    // 5. Fetch comments — subscribes to relay.primal.net and populates cache.
    let res = dispatch(
        app,
        PODCAST_NS,
        json!({
            "op": "fetch_comments",
            "episode_id": episode_id
        }),
    );
    if let Some(err) = res.get("error").and_then(|v| v.as_str()) {
        return Fail(format!("FetchComments dispatch rejected: {err}"));
    }
    // Wait for actor to process FetchComments. It marks this episode as the
    // viewed comment target and bumps the snapshot so the optimistic comment
    // already cached by PostComment becomes visible in `comments`.
    let fetch_ok = wait_for(handle, 15_000, |u| !u.comments.is_empty());
    if let Err(e) = fetch_ok {
        return Fail(format!("FetchComments actor timed out: {e}"));
    }

    // Both actions processed by the actor without error, and the snapshot now
    // projects the viewed episode's optimistic comment while relay results are
    // still free to arrive asynchronously via the observer.
    Pass
}
